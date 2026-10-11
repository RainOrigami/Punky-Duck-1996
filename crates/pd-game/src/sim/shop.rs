//! Shop rules (`shop_check`, `0x2a450`). The modal screen (picture, cursor, hot zones)
//! belongs to the app; this module holds what a purchase does.
//! Spec: `docs/re/game/items-and-scripts.md` 10.
//!
//! The original keeps "undo" copies of the purchasable state. Each successful or
//! attempted purchase overwrites the copy of the field it touches with the value from
//! just before that purchase, so **cancel only undoes the last purchase of each kind**
//! and refunds only the most recent price. Reproduced as coded.

use super::Sim;

/// Product selection `sel` = hot zone id - 3: 0 energy, 1 fuel, 2 compass, 3 power 1,
/// 4 power 2, 5/7/../17 weapons 1..7, 6/8/../18 their ammo.
pub const PRODUCTS: usize = 19;

/// `{BONUS_n.SPT frame, price}` per product (`0x27d40`).
pub const PRODUCT_TABLE: [(i32, i32); PRODUCTS] = [
    (0, 15000),
    (34, 30000),
    (37, 30000),
    (38, 20000),
    (39, 40000),
    (40, 10000),
    (41, 1000),
    (42, 15000),
    (43, 1500),
    (44, 30000),
    (45, 3000),
    (46, 40000),
    (47, 4000),
    (48, 60000),
    (49, 6000),
    (50, 70000),
    (51, 7000),
    (52, 90000),
    (53, 9000),
];

const AMMO_PACK: [i32; 8] = [0, 200, 150, 100, 50, 50, 50, 10];

/// Result of pressing BUY, i.e. which message the original shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuyResult {
    /// "OK", sample 14.
    Ok,
    /// "Not enough money", sample 25; state restored from the undo copies.
    NotEnoughMoney,
    /// "not useful", sample 25; state restored from the undo copies.
    NotUseful,
}

/// The undo copies made when the shop opens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shop {
    owned: [i32; 10],
    ammo: [i32; 10],
    fuel: i32,
    compass: i32,
    money: i32,
    power_shots: i32,
    energy: i32,
    power: i32,
}

impl Shop {
    pub fn open(sim: &Sim) -> Self {
        let p = &sim.s.p;
        Shop {
            owned: p.owned,
            ammo: p.ammo,
            fuel: p.fuel,
            compass: p.compass,
            money: p.money,
            power_shots: p.power_shots,
            energy: p.energy,
            power: p.power,
        }
    }

    fn restore(&self, sim: &mut Sim) {
        let p = &mut sim.s.p;
        p.owned = self.owned;
        p.ammo = self.ammo;
        p.fuel = self.fuel;
        p.compass = self.compass;
        p.money = self.money;
        p.power_shots = self.power_shots;
        p.energy = self.energy;
        p.power = self.power;
    }

    /// CANCEL: restore the undo copies ("Cancel!!!").
    pub fn cancel(&self, sim: &mut Sim) {
        self.restore(sim);
    }

    /// BUY with product `sel` selected.
    pub fn buy(&mut self, sim: &mut Sim, sel: usize) -> BuyResult {
        let mut price = 0;
        let mut samples: Vec<i32> = Vec::new();
        {
            let p = &mut sim.s.p;
            match sel {
                0 if p.energy < 90 => {
                    price = 15000;
                    self.energy = p.energy;
                    p.energy = 99;
                    samples.push(27);
                }
                1 if p.fuel < 4500 => {
                    price = 30000;
                    self.fuel = p.fuel;
                    p.fuel = 5000;
                    samples.push(5);
                }
                2 if p.compass == 0 => {
                    price = 30000;
                    self.compass = p.compass;
                    p.compass = 1;
                    samples.push(0);
                }
                3 => {
                    if p.power == 0 {
                        price = 20000;
                        self.power = p.power;
                        p.power = 1;
                        p.power_shots = 400;
                        samples.push(23);
                    }
                    if p.power == 1 && p.power_shots < 300 {
                        price = 20000;
                        self.power = p.power;
                        p.power_shots = 400;
                        samples.push(23);
                    }
                }
                4 => {
                    // Three tests in sequence; the price is assigned, not added.
                    if p.power == 0 {
                        price = 40000;
                        self.power = p.power;
                        p.power = 2;
                        p.power_shots = 400;
                        samples.push(23);
                    }
                    if p.power == 1 {
                        price = 40000;
                        self.power = p.power;
                        p.power = 2;
                        p.power_shots = 400;
                        samples.push(23);
                    }
                    if p.power == 2 && p.power_shots < 300 {
                        price = 40000;
                        self.power = p.power;
                        p.power_shots = 400;
                        samples.push(23);
                    }
                }
                5 | 7 | 9 | 11 | 13 | 15 | 17 => {
                    let w = (sel - 3) / 2;
                    if p.owned[w] == 0 {
                        price = PRODUCT_TABLE[sel].1;
                        self.owned[w] = p.owned[w];
                        p.owned[w] = 1;
                        samples.push(23);
                    }
                }
                6 | 8 | 10 | 12 | 14 | 16 | 18 => {
                    let w = (sel - 4) / 2;
                    self.ammo[w] = p.ammo[w];
                    p.ammo[w] += AMMO_PACK[w];
                    price = PRODUCT_TABLE[sel].1;
                    samples.push(23);
                }
                _ => {}
            }
        }
        for n in samples {
            sim.sfx(n, 255);
        }
        if sim.s.p.money < price {
            self.restore(sim);
            sim.sfx(25, 255);
            BuyResult::NotEnoughMoney
        } else if price != 0 {
            sim.sfx(14, 255);
            self.money = sim.s.p.money;
            sim.s.p.money -= price;
            BuyResult::Ok
        } else {
            self.restore(sim);
            sim.sfx(25, 255);
            BuyResult::NotUseful
        }
    }
}

impl Sim {
    /// Leaving the shop: the duck is moved straight backwards out of zone 18.
    pub fn shop_exit(&mut self) {
        self.eject_from(18);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Enhancements;

    fn sim() -> Sim {
        let mut s = Sim::new(Enhancements::original());
        s.new_game();
        s
    }

    #[test]
    fn energy_and_money() {
        let mut s = sim();
        s.s.p.energy = 50;
        s.s.p.money = 20000;
        let mut shop = Shop::open(&s);
        assert_eq!(shop.buy(&mut s, 0), BuyResult::Ok);
        assert_eq!((s.s.p.energy, s.s.p.money), (99, 5000));
        // energy now >= 90
        assert_eq!(shop.buy(&mut s, 0), BuyResult::NotUseful);
        // "not useful" restores the undo copies: energy back to 50, money to the
        // value before the last successful purchase.
        assert_eq!((s.s.p.energy, s.s.p.money), (50, 20000));
    }

    #[test]
    fn not_enough_money_restores() {
        let mut s = sim();
        s.s.p.money = 500;
        let mut shop = Shop::open(&s);
        assert_eq!(shop.buy(&mut s, 5), BuyResult::NotEnoughMoney);
        assert_eq!(s.s.p.owned[1], 0);
        assert_eq!(s.s.p.money, 500);
    }

    #[test]
    fn weapons_and_ammo() {
        let mut s = sim();
        s.s.p.money = 200_000;
        let mut shop = Shop::open(&s);
        assert_eq!(shop.buy(&mut s, 17), BuyResult::Ok);
        assert_eq!(s.s.p.owned[7], 1);
        assert_eq!(s.s.p.money, 110_000);
        assert_eq!(shop.buy(&mut s, 18), BuyResult::Ok);
        assert_eq!(s.s.p.ammo[7], 10);
        assert_eq!(shop.buy(&mut s, 6), BuyResult::Ok);
        assert_eq!(s.s.p.ammo[1], 200);
        assert_eq!(s.s.p.money, 110_000 - 9000 - 1000);
        assert_eq!(shop.buy(&mut s, 17), BuyResult::NotUseful);
    }

    #[test]
    fn power_levels() {
        let mut s = sim();
        s.s.p.money = 100_000;
        let mut shop = Shop::open(&s);
        assert_eq!(shop.buy(&mut s, 4), BuyResult::Ok);
        assert_eq!((s.s.p.power, s.s.p.power_shots, s.s.p.money), (2, 400, 60_000));
        assert_eq!(shop.buy(&mut s, 3), BuyResult::NotUseful);
    }
}
