//! Constant tables lifted from `P.EXE` (addresses in the item docs).

/// Weapon cooldown in steps, `[weapon][power]` (`0x419b0`).
pub const COOLDOWN: [[i32; 3]; 10] = [
    [25, 15, 8],
    [25, 15, 8],
    [25, 15, 8],
    [25, 15, 8],
    [25, 15, 8],
    [25, 15, 8],
    [25, 15, 8],
    [200, 150, 100],
    [0, 0, 0],
    [0, 0, 0],
];

/// `bonus_frames` initial contents (`0x403b4`, copied to `0x5167c8` on every island
/// entry): `{first, last}` frame of `BONUS_n.SPT` per bonus kind.
pub fn bonus_frames_init() -> [[i32; 2]; 100] {
    let mut t = [[0i32; 2]; 100];
    t[0] = [0, 9];
    t[1] = [10, 13];
    t[2] = [14, 23];
    t[3] = [24, 33];
    for (k, e) in t.iter_mut().enumerate().take(47).skip(4) {
        *e = [k as i32 + 30, k as i32 + 30];
    }
    t
}

/// Monster sprite tables (`0x4105c + 0x2c * k`), `[island - 1][type - 1]`, type 5 = boss:
/// `{base, walk_off, attack_off, stand_off, death_off, shot_off, walk_n, attack_n,
/// stand_n, death_n, shot_n}`.
pub const MONSTER_SPRITES: [[[i32; 11]; 5]; 5] = [
    [
        [0, 0, 30, 45, 50, 54, 6, 3, 1, 4, 1],
        [59, 0, 5, 25, 30, 35, 1, 4, 1, 5, 1],
        [99, 0, 5, 15, 20, 23, 1, 2, 1, 3, 1],
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        [0, 0, 10, 15, 25, 32, 2, 1, 2, 7, 1],
    ],
    [
        [0, 0, 5, 15, 20, 23, 1, 2, 1, 3, 1],
        [28, 0, 5, 10, 15, 18, 1, 1, 1, 3, 1],
        [51, 0, 20, 30, 50, 57, 4, 2, 4, 7, 1],
        [113, 0, 10, 15, 25, 32, 2, 1, 2, 7, 1],
        [0, 0, 5, 20, 25, 30, 1, 3, 1, 5, 4],
    ],
    [
        [0, 0, 5, 15, 20, 24, 1, 2, 1, 4, 1],
        [29, 0, 5, 5, 10, 15, 1, 0, 1, 5, 0],
        [44, 0, 20, 30, 35, 42, 4, 2, 1, 7, 1],
        [91, 0, 5, 25, 30, 35, 1, 4, 1, 5, 1],
        [0, 0, 20, 25, 45, 50, 4, 1, 4, 5, 1],
    ],
    [
        [0, 0, 5, 10, 15, 18, 1, 1, 1, 3, 1],
        [23, 0, 20, 25, 30, 34, 4, 1, 1, 4, 4],
        [77, 0, 25, 25, 30, 37, 5, 0, 1, 7, 0],
        [114, 0, 20, 25, 45, 50, 4, 1, 4, 5, 1],
        [0, 0, 40, 60, 65, 71, 8, 4, 1, 6, 1],
    ],
    [
        [0, 0, 35, 35, 45, 51, 7, 0, 2, 6, 0],
        [51, 0, 20, 25, 45, 51, 4, 1, 4, 6, 1],
        [107, 0, 5, 5, 35, 36, 1, 0, 6, 1, 0],
        [143, 0, 20, 25, 30, 36, 4, 1, 1, 6, 1],
        [0, 0, 0, 0, 20, 27, 4, 4, 4, 7, 1],
    ],
];

/// Monster movement class per type 1..4 (`0x4159c..0x4159f`), set by the island reset
/// functions `0x215b0`.. : 0 walker, 1 flyer, 2 swimmer, 3 walker (melee).
pub const CLASS_OF: [[u8; 4]; 5] =
    [[0, 1, 0, 0], [0, 1, 0, 1], [0, 2, 0, 1], [0, 0, 2, 1], [2, 1, 3, 0]];

/// Boss shot origin offset, `0x415a0[island]` (i32). Entry 0 is unused by this lookup;
/// its low byte doubles as `class_of[5]` and is set to 1 when a boss is created.
pub const BOSS_MUZZLE: [i32; 6] = [0, 0, 0, 8000, 30000, 50000];

/// Position smoothing divisor per island (`0x517f84`, set by the asset functions).
pub const SLIDE: [i32; 5] = [8, 8, 15, 40, 8];

/// Player start per island in stored (negated) coordinates.
pub const START: [(i32, i32); 5] =
    [(-19650, -10575), (25087, 20362), (8100, 19387), (-24412, -25387), (-4050, -22500)];

/// One `item_add(id, x, y, z, icon, p6)` call of an island start function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemDef {
    pub id: i32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub icon: i32,
    pub p6: i32,
}

const fn it(id: i32, x: i32, y: i32, z: i32, icon: i32, p6: i32) -> ItemDef {
    ItemDef { id, x, y, z, icon, p6 }
}

const ITEMS_1: &[ItemDef] = &[
    it(0, 18562, -300, 1687, 0, 200),
    it(1, 19724, -300, 5864, 1, 300),
    it(2, -13500, -300, -25500, 2, 300),
    it(3, 3000, -300, -600, 3, 500),
    it(4, -28930, -300, -6818, 4, 500),
    it(5, 21000, -300, 23400, 5, 600),
    it(6, -13500, -600, 25500, 6, 1000),
    it(15, 31200, -300, 15000, 15, 0),
    it(17, -21300, -300, -12900, 17, 0),
];

const ITEMS_2: &[ItemDef] = &[
    it(0, -19500, -300, -14400, 0, 200),
    it(1, -21637, -300, 11737, 0, 200),
    it(2, 24543, -300, 1415, 0, 200),
    it(4, 15590, -300, -25209, 0, 200),
    it(6, 19743, -300, 27150, 0, 200),
    it(5, -30862, -300, -12637, 0, 200),
    it(7, 6356, -300, 18356, 0, 200),
    it(8, -27290, -300, -2034, 0, 200),
    it(9, -6787, -300, 24075, 0, 200),
    it(10, -15900, -300, -26775, 0, 200),
    it(11, -4846, -300, 468, 0, 200),
    it(13, -581, -300, -30543, 13, 200),
    it(15, -33881, -300, 29062, 15, 200),
    it(17, 19368, -300, -28171, 17, 200),
];

const ITEMS_3: &[ItemDef] = &[
    it(5, 1162, -300, -29925, 5, 200),
    it(3, 6112, -300, -3637, 3, 200),
    it(8, 13537, -300, -27975, 5, 200),
    it(0, 18675, -300, 26100, 5, 200),
    it(4, 27900, -300, -12975, 5, 200),
    it(9, -5718, -300, -18900, 5, 200),
    it(10, -5700, -300, 15225, 5, 200),
    it(1, 8850, -300, 12975, 5, 200),
    it(2, 24900, -300, 8400, 5, 200),
    it(11, -23550, -300, 900, 5, 200),
    it(7, -10518, -300, 21562, 5, 200),
    it(12, 150, -300, -25500, 5, 200),
    it(13, 5250, -300, -13200, 5, 200),
    it(14, -10509, -300, -9553, 14, 200),
    it(16, 23390, -300, 23371, 16, 200),
    it(18, -20671, -300, -928, 18, 200),
    it(20, 2250, -300, 4781, 20, 200),
    it(22, -4218, -300, 27628, 22, 200),
];

const ITEMS_4: &[ItemDef] = &[
    it(8, -8100, -300, 21187, 8, 0),
    it(9, -24487, -300, -3525, 8, 0),
    it(6, -25800, -300, -16200, 8, 0),
    it(10, 22125, -300, 31425, 8, 0),
    it(4, -3900, -300, -15525, 8, 0),
    it(11, -1800, -300, -13000, 8, 0),
    it(5, 28143, -300, -21318, 8, 0),
    it(3, 24675, -300, 25125, 8, 0),
    it(1, 24975, -300, -12375, 8, 0),
    it(2, -7425, -300, -4275, 8, 0),
    it(7, 1453, -300, -19621, 8, 0),
    it(0, -20475, -300, 5325, 8, 0),
    it(12, -7500, -300, 10500, 8, 0),
    it(13, -24000, -300, 12300, 8, 0),
    it(14, 23700, -300, 13200, 8, 0),
    it(15, 9093, -300, -3693, 8, 0),
    it(16, 28462, -300, 9862, 16, 200),
    it(18, -20175, -300, 18862, 18, 200),
    it(20, 15862, -300, -12375, 20, 200),
    it(22, 22931, -300, -5268, 22, 200),
    it(24, -20981, -300, -15393, 24, 200),
];

const ITEMS_5: &[ItemDef] = &[
    it(0, -7950, -300, 2550, 0, 0),
    it(1, 23175, -300, 3000, 0, 0),
    it(2, 28500, -300, 19425, 0, 0),
    it(3, -25650, -300, 22200, 0, 0),
    it(3, -10575, -300, -31125, 0, 0),
    it(3, 13425, -300, -24600, 0, 0),
    it(3, 30075, -300, -16200, 0, 0),
    it(4, -12900, -300, 14475, 0, 0),
    it(4, -31800, -300, -12075, 0, 0),
    it(4, -5625, -300, -2550, 0, 0),
    it(4, 18750, -300, 10050, 0, 0),
    it(5, -900, -300, -28725, 0, 0),
    it(5, 13350, -300, -10275, 0, 0),
    it(5, 150, -300, 16575, 0, 0),
    it(5, -19800, -300, -21375, 0, 0),
    it(6, -15150, -300, 6600, 0, 0),
    it(7, -24975, -300, -9450, 0, 0),
    it(9, -8700, -300, 26850, 0, 0),
    it(10, 18281, -300, -4125, 10, 200),
    it(12, 15825, -300, -3225, 12, 200),
    it(14, 30075, -300, 5362, 14, 200),
    it(16, -7875, -300, 7050, 16, 200),
    it(18, 15900, -300, -21750, 18, 200),
    it(20, -15375, -300, 2475, 20, 200),
];

/// Quest items placed by the island start functions (`0x17680`, `0x19030`, `0x1aa80`,
/// `0x1c760`, `0x15ce0`), in call order.
pub fn island_items(island: i32) -> &'static [ItemDef] {
    match island {
        1 => ITEMS_1,
        2 => ITEMS_2,
        3 => ITEMS_3,
        4 => ITEMS_4,
        5 => ITEMS_5,
        _ => &[],
    }
}

/// Extra bonus entry appended after the random money by the island start function:
/// `(kind, x, y, z)`. Island 1: the compass; island 3: decor kind 45.
pub fn island_extra_bonus(island: i32) -> Option<(i32, i32, i32, i32)> {
    match island {
        1 => Some((7, 19950, -3000, 3300)),
        3 => Some((45, -2315, -155, 150)),
        _ => None,
    }
}

/// How a decor kind (bonus kind >= 24) reacts to the duck on an island.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decor {
    /// Pass-through decoration.
    None,
    /// Blocks: `bump = flying ? 2 : 3`, sample 21 on foot.
    Solid,
    /// `energy -= 10; hurt = 20`.
    Harm,
    /// Same, only on foot.
    HarmOnFoot,
}

/// Decor class per island, from the five `pickups_update` copies (`0x18160`, `0x1a200`,
/// `0x1be60`, `0x1dbb0`, `0x16f90`).
pub fn decor_class(island: i32, kind: i32) -> Decor {
    match island {
        1 => match kind {
            29 | 30 | 31 | 44 | 45 => Decor::Solid,
            24 | 26 | 28 | 34 => Decor::Harm,
            27 => Decor::HarmOnFoot,
            _ => Decor::None,
        },
        2 => match kind {
            26 | 27 | 28 | 36..=44 => Decor::Solid,
            25 | 31 | 32 => Decor::Harm,
            24 => Decor::HarmOnFoot,
            _ => Decor::None,
        },
        3 => match kind {
            26..=42 => Decor::Solid,
            _ => Decor::None,
        },
        4 => match kind {
            24..=33 => Decor::Solid,
            36..=39 => Decor::Harm,
            _ => Decor::None,
        },
        5 => match kind {
            26 | 27 | 28 | 31 | 32 | 33 | 38 => Decor::Solid,
            24 | 25 | 29 | 30 | 34 | 35 | 36 | 37 => Decor::Harm,
            _ => Decor::None,
        },
        _ => Decor::None,
    }
}

/// A fixed scenery mesh queued every frame by the island frame function:
/// `(slot, model, x, z, yaw)`; y is 0.
pub type FixedMesh = (i32, i32, f32, f32, f32);

const YAW: f32 = -1.5708;

/// Fixed meshes per island (`0x187e0`, `0x1a6d0`, `0x1c450`, `0x1e080`). Island 2 slot 6
/// is only drawn while `flag70 == 0` (handled by the caller). Island 5 has none besides
/// the four vessels its script places.
pub fn fixed_meshes(island: i32) -> &'static [FixedMesh] {
    match island {
        1 => &[
            (0, 4, 15881.0, 3225.0, 0.0),
            (1, 5, 18318.0, 2700.0, 0.0),
            (2, 4, 18318.0, 4481.0, 0.0),
            (3, 5, 20662.0, 2981.0, 0.0),
            (5, 6, -11868.0, -25050.0, 0.0),
            (7, 7, -31600.0, 26850.0, 0.0),
        ],
        2 => &[
            (1, 8, -1500.0, 11700.0, 0.0),
            (2, 8, 1200.0, 11400.0, 0.0),
            (3, 8, -1800.0, 14100.0, 0.0),
            (4, 9, -600.0, 12900.0, 0.0),
            (5, 9, 2700.0, 12000.0, 0.0),
            (6, 7, -34030.0, 27600.0, 0.0),
            (7, 6, -24900.0, -21900.0, 0.0),
            (8, 10, -29100.0, -7900.0, 0.0),
        ],
        3 => &[
            (2, 5, 1200.0, -28500.0, 0.0),
            (3, 5, 25930.0, 7950.0, YAW),
            (4, 6, 16100.0, 26250.0, 0.0),
        ],
        4 => &[(2, 2, -23000.0, 30440.0, 0.0)],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bonus_frames() {
        let t = bonus_frames_init();
        assert_eq!(t[2], [14, 23]);
        assert_eq!(t[4], [34, 34]);
        assert_eq!(t[46], [76, 76]);
        assert_eq!(t[47], [0, 0]);
    }

    #[test]
    fn decor_classes() {
        assert_eq!(decor_class(1, 29), Decor::Solid);
        assert_eq!(decor_class(1, 27), Decor::HarmOnFoot);
        assert_eq!(decor_class(2, 44), Decor::Solid);
        assert_eq!(decor_class(3, 44), Decor::None);
        assert_eq!(decor_class(5, 37), Decor::Harm);
        assert_eq!(decor_class(4, 34), Decor::None);
    }
}
