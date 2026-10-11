//! The game's pseudo-random generator: Watcom C `rand()` (`0x30570`).
//!
//! `seed = seed * 0x41c64e6d + 0x3039; return (seed >> 16) & 0x7fff`. The seed lives at
//! `0x41c60`, starts at 1 and is never re-seeded (`srand` at `0x30593` has no caller), so
//! it runs on from program start across menus, islands and loaded games. It is not part
//! of a save game.
//!
//! Callers, all in gameplay code: random money placement at island start (`0x28bc0`),
//! one call per rendered frame for the animated tiles (`0x11b40`), monster respawn rolls,
//! monster notice sound, monster fire rolls and scatter (`0x21af0`), body contact money
//! loss (`0x23ec0`), player shot speed and cluster explosions (`0x103b0`).

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rng {
    pub seed: u32,
}

impl Rng {
    /// State at program start.
    pub fn new() -> Self {
        Rng { seed: 1 }
    }

    /// `rand()`: 0..=0x7fff.
    pub fn next(&mut self) -> i32 {
        self.seed = self.seed.wrapping_mul(0x41c6_4e6d).wrapping_add(0x3039);
        ((self.seed >> 16) & 0x7fff) as i32
    }
}

impl Default for Rng {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watcom_sequence() {
        // Known values of the 0x41c64e6d / 12345 generator from seed 1.
        let mut r = Rng::new();
        assert_eq!(r.next(), 16838);
        assert_eq!(r.next(), 5758);
        assert_eq!(r.next(), 10113);
        assert_eq!(r.next(), 17515);
    }
}
