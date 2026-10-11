//! `ILE0n.TRJ` and `MONSTERn.TRJ` — placed items with optional paths.
//!
//! Spec: `docs/re/formats/trj.md`. Both file families share one layout:
//!
//! ```text
//! u32 path_count;   path_count   x { u32 first_point, u32 point_count }
//! u32 record_count; record_count x 32-byte record
//! u32 point_count;  point_count  x { f32 x, f32 z }
//! ```
//!
//! `ILE0n.TRJ` lists the bonuses of an island (loader `P.EXE` `0x27fd0`, which
//! uses only `kind`, `x`, `z` of each record). `MONSTERn.TRJ` lists the
//! monsters (loader `0x24000`, spawn code `0x24110`), which also use the path,
//! scale and angle.

use std::fmt;

/// Record size in bytes.
pub const RECORD_SIZE: usize = 32;
/// Bonus loader limit: `record_count < 1600` ("Erreur nombre de bonus").
pub const MAX_BONUS_RECORDS: usize = 0x640;
/// Bonus loader limit: `0 <= kind <= 54` ("Erreur BONUS").
pub const MAX_BONUS_KIND: i32 = 0x36;
/// Monster loader limit: `record_count < 210` ("Internal Error : BAD TRJ").
pub const MAX_MONSTER_RECORDS: usize = 0xd2;
/// Monster loader reads at most this many bytes of the file.
pub const MAX_MONSTER_FILE: usize = 0x2800;
/// World Y the bonus loader gives every bonus (`0x28070`).
pub const BONUS_Y: i32 = -600;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrjError(pub String);

impl fmt::Display for TrjError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "TRJ: {}", self.0)
    }
}

impl std::error::Error for TrjError {}

/// A path: a run of consecutive entries of [`Trj::points`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Path {
    pub first_point: u32,
    pub point_count: u32,
}

/// One placed bonus or monster.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Record {
    /// Offset 0. 32 in every disc record; not read by the game.
    pub field0: i32,
    /// Offset 4. Index into [`Trj::paths`]. Used for monsters only.
    pub path: i32,
    /// Offset 8. 0 in every disc record; not read by the game.
    pub field8: i32,
    /// Offset 12. World X.
    pub x: i32,
    /// Offset 16. World Z.
    pub z: i32,
    /// Offset 20. Path scale. Used for monsters only.
    pub scale: f32,
    /// Offset 24. Path rotation in radians. Used for monsters only.
    pub angle: f32,
    /// Offset 28. Bonus type (0..=54), or monster type minus one.
    pub kind: i32,
}

/// A parsed `.TRJ`.
#[derive(Debug, Clone)]
pub struct Trj {
    pub paths: Vec<Path>,
    pub records: Vec<Record>,
    /// Path points `[x, z]` in path-local units.
    pub points: Vec<[f32; 2]>,
}

fn u32_at(d: &[u8], o: usize) -> Result<u32, TrjError> {
    d.get(o..o + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| TrjError(format!("truncated at offset {o}")))
}

/// The game's float to int conversion (`0x305a4`): rounding control forced
/// to truncation, then `frndint` / `fistp`.
fn chop(v: f32) -> i32 {
    v as i32
}

impl Trj {
    pub fn parse(data: &[u8]) -> Result<Self, TrjError> {
        let mut o = 0usize;
        let path_count = u32_at(data, o)? as usize;
        o += 4;
        if path_count > data.len() / 8 {
            return Err(TrjError(format!("{path_count} paths exceed the file")));
        }
        let mut paths = Vec::with_capacity(path_count);
        for i in 0..path_count {
            paths.push(Path {
                first_point: u32_at(data, o + i * 8)?,
                point_count: u32_at(data, o + i * 8 + 4)?,
            });
        }
        o += path_count * 8;

        let record_count = u32_at(data, o)? as usize;
        o += 4;
        if record_count > data.len() / RECORD_SIZE {
            return Err(TrjError(format!("{record_count} records exceed the file")));
        }
        let mut records = Vec::with_capacity(record_count);
        for i in 0..record_count {
            let b = o + i * RECORD_SIZE;
            let w = |k: usize| u32_at(data, b + k);
            records.push(Record {
                field0: w(0)? as i32,
                path: w(4)? as i32,
                field8: w(8)? as i32,
                x: w(12)? as i32,
                z: w(16)? as i32,
                scale: f32::from_bits(w(20)?),
                angle: f32::from_bits(w(24)?),
                kind: w(28)? as i32,
            });
        }
        o += record_count * RECORD_SIZE;

        let point_count = u32_at(data, o)? as usize;
        o += 4;
        if point_count > data.len() / 8 {
            return Err(TrjError(format!("{point_count} points exceed the file")));
        }
        let mut points = Vec::with_capacity(point_count);
        for i in 0..point_count {
            points.push([
                f32::from_bits(u32_at(data, o + i * 8)?),
                f32::from_bits(u32_at(data, o + i * 8 + 4)?),
            ]);
        }
        Ok(Trj { paths, records, points })
    }

    /// Points of the path a record refers to; empty when out of range.
    pub fn path_points(&self, record: &Record) -> &[[f32; 2]] {
        let Some(p) = usize::try_from(record.path).ok().and_then(|i| self.paths.get(i)) else {
            return &[];
        };
        let a = p.first_point as usize;
        self.points.get(a..a + p.point_count as usize).unwrap_or(&[])
    }

    /// Monster spawn position and first waypoint in world `(x, z)`, as
    /// computed at `0x2440c`..`0x2459c`: the first path point (truncated to
    /// int) is the pivot, the second point is rotated and scaled around it,
    /// and the record position is added to both.
    pub fn monster_start(&self, record: &Record) -> ((i32, i32), (i32, i32)) {
        let pts = self.path_points(record);
        let Some(p0) = pts.first() else {
            return ((record.x, record.z), (record.x, record.z));
        };
        let pivot = (chop(p0[0]), chop(p0[1]));
        let target = if pts.len() > 1 { next_waypoint(record, pivot, pts[1]) } else { pivot };
        (
            (pivot.0 + record.x, pivot.1 + record.z),
            (target.0 + record.x, target.1 + record.z),
        )
    }
}

/// The waypoint formula shared by the monster spawn code (`0x2445b`) and the
/// "waypoint reached" code (`0x230a7`), transcribed literally:
///
/// ```text
/// tx = trunc(((pz - cz) * sin(a) + (px - cx) * cos(a)) * scale + cx)
/// tz = trunc(((pz - cz) * cos(a) - (px - cx) * sin(a)) * scale + cz)
/// ```
///
/// `current` is the integer position the monster is at, `point` the raw path
/// point it heads for next.
pub fn next_waypoint(record: &Record, current: (i32, i32), point: [f32; 2]) -> (i32, i32) {
    let (s, c) = (record.angle.sin(), record.angle.cos());
    let (cx, cz) = (current.0 as f32, current.1 as f32);
    let dx = point[0] - cx;
    let dz = point[1] - cz;
    (
        chop((dz * s + dx * c) * record.scale + cx),
        chop((dz * c - dx * s) * record.scale + cz),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disc(name: &str) -> Option<Vec<u8>> {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../disc/extracted/PUNKY/");
        std::fs::read(format!("{p}{name}")).ok()
    }

    fn sample() -> Vec<u8> {
        let mut d = Vec::new();
        for v in [1u32, 0, 2, 1] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        for v in [32i32, 0, 0, 1000, -2000] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d.extend_from_slice(&0.5f32.to_le_bytes());
        d.extend_from_slice(&0f32.to_le_bytes());
        d.extend_from_slice(&7i32.to_le_bytes());
        d.extend_from_slice(&2u32.to_le_bytes());
        for v in [0f32, 0., 10., 20.] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        d
    }

    #[test]
    fn synthetic() {
        let d = sample();
        let t = Trj::parse(&d).unwrap();
        let r = t.records[0];
        assert_eq!((r.x, r.z, r.kind, r.scale), (1000, -2000, 7, 0.5));
        assert_eq!(t.path_points(&r).len(), 2);
        assert_eq!(t.monster_start(&r), ((1000, -2000), (1005, -1990)));
        assert!(Trj::parse(&d[..d.len() - 1]).is_err());
    }

    #[test]
    fn disc_bonus_files() {
        for n in 1..=5 {
            let Some(d) = disc(&format!("ILE0{n}.TRJ")) else {
                eprintln!("disc files absent, skipping");
                return;
            };
            let t = Trj::parse(&d).unwrap();
            assert_eq!(d.len(), 12 + t.paths.len() * 8 + t.records.len() * 32 + t.points.len() * 8);
            assert!(t.records.len() < MAX_BONUS_RECORDS);
            for r in &t.records {
                assert!((0..=MAX_BONUS_KIND).contains(&r.kind));
                assert_eq!((r.field0, r.field8, r.path), (32, 0, 0));
                assert_eq!((r.scale, r.angle), (1.0, 0.0));
            }
        }
    }

    #[test]
    fn disc_monster_files() {
        for n in 1..=5 {
            let Some(d) = disc(&format!("MONSTER{n}.TRJ")) else {
                eprintln!("disc files absent, skipping");
                return;
            };
            assert!(d.len() <= MAX_MONSTER_FILE);
            let t = Trj::parse(&d).unwrap();
            assert_eq!(d.len(), 12 + t.paths.len() * 8 + t.records.len() * 32 + t.points.len() * 8);
            assert!(t.records.len() < MAX_MONSTER_RECORDS);
            for p in &t.paths {
                assert!((p.first_point + p.point_count) as usize <= t.points.len());
            }
            for r in &t.records {
                assert_eq!((r.field0, r.field8), (32, 0));
                assert!((r.path as usize) < t.paths.len());
                assert!((0..=3).contains(&r.kind));
            }
        }
    }
}
