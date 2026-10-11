//! `OBJETS_n.3D` — the 3D objects of an island.
//!
//! Spec: `docs/re/formats/3d.md`. Follows the first half of the loader at
//! `P.EXE` `0x141a0`:
//!
//! ```text
//! u32 object_count, u32 point_count, u32 face_count, u32 unused
//! object_count x { u32 first_point, u32 first_face, u32 point_count, u32 face_count }
//! point_count  x { f32 x, f32 y, f32 z }
//! face_count   x 24-byte face
//! ```
//!
//! Points are stored at half size: the loader multiplies every coordinate by
//! 2.0 (`0x42457`). File Y points down (the renderer negates it, `0x13e7e`).

use std::fmt;

/// Loader limit: `point_count * 3 <= 8400` (`0x142ae`).
pub const MAX_POINTS: usize = 8400 / 3;
/// Loader limit: `face_count * 3 <= 6500` (`0x142f6`).
pub const MAX_FACES: usize = 6500 / 3;
/// Factor the loader applies to every stored coordinate.
pub const POINT_SCALE: f32 = 2.0;

const FACE_SIZE: usize = 24;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mesh3dError(pub String);

impl fmt::Display for Mesh3dError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "3D: {}", self.0)
    }
}

impl std::error::Error for Mesh3dError {}

/// One object: a range of points and a range of faces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Object {
    pub first_point: u32,
    pub first_face: u32,
    pub point_count: u32,
    pub face_count: u32,
}

/// One textured triangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Face {
    /// Index into the texture table of `OBJETS_n.MAP`.
    pub texture: i16,
    /// Copy of the owning object's `first_point`. Not read by the game.
    pub point_base: i16,
    /// Always 0. Not read by the game.
    pub unused: i16,
    /// Point indices relative to the owning object's first point.
    pub points: [i16; 3],
    /// Texel column per corner, relative to the texture's top-left.
    pub u: [i16; 3],
    /// Texel row per corner.
    pub v: [i16; 3],
}

/// A parsed `OBJETS_n.3D`.
#[derive(Debug, Clone)]
pub struct Mesh3d {
    /// Fourth header word. Read and discarded by the game; 0 on disc.
    pub header_unused: u32,
    pub objects: Vec<Object>,
    /// Points exactly as stored (not yet multiplied by [`POINT_SCALE`]).
    pub points: Vec<[f32; 3]>,
    pub faces: Vec<Face>,
}

fn u32_at(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]])
}

fn i16_at(d: &[u8], o: usize) -> i16 {
    i16::from_le_bytes([d[o], d[o + 1]])
}

impl Mesh3d {
    pub fn parse(data: &[u8]) -> Result<Self, Mesh3dError> {
        if data.len() < 16 {
            return Err(Mesh3dError("file shorter than the header".into()));
        }
        let object_count = u32_at(data, 0) as usize;
        let point_count = u32_at(data, 4) as usize;
        let face_count = u32_at(data, 8) as usize;
        if point_count > MAX_POINTS {
            // "Erreur nb pts dans objets 3d %d %d"
            return Err(Mesh3dError(format!("{point_count} points exceed the game limit")));
        }
        if face_count > MAX_FACES {
            // "Erreur nb faces dans objets 3d %d %d"
            return Err(Mesh3dError(format!("{face_count} faces exceed the game limit")));
        }
        let points_at = object_count
            .checked_mul(16)
            .and_then(|n| n.checked_add(16))
            .ok_or_else(|| Mesh3dError("object table size overflows".into()))?;
        let faces_at = points_at + point_count * 12;
        let end = faces_at + face_count * FACE_SIZE;
        if data.len() < end {
            return Err(Mesh3dError(format!(
                "file has {} bytes, sections need {end}",
                data.len()
            )));
        }

        let objects: Vec<Object> = (0..object_count)
            .map(|i| {
                let o = 16 + i * 16;
                Object {
                    first_point: u32_at(data, o),
                    first_face: u32_at(data, o + 4),
                    point_count: u32_at(data, o + 8),
                    face_count: u32_at(data, o + 12),
                }
            })
            .collect();
        let points = (0..point_count)
            .map(|i| {
                let o = points_at + i * 12;
                [0, 4, 8].map(|k| f32::from_bits(u32_at(data, o + k)))
            })
            .collect();
        let faces = (0..face_count)
            .map(|i| {
                let o = faces_at + i * FACE_SIZE;
                let w = |k: usize| i16_at(data, o + k);
                Face {
                    texture: w(0),
                    point_base: w(2),
                    unused: w(4),
                    points: [w(6), w(8), w(10)],
                    u: [w(12), w(14), w(16)],
                    v: [w(18), w(20), w(22)],
                }
            })
            .collect();
        Ok(Mesh3d { header_unused: u32_at(data, 12), objects, points, faces })
    }

    /// Points of one object as stored; empty when the range is out of bounds.
    pub fn object_points(&self, object: usize) -> &[[f32; 3]] {
        let Some(o) = self.objects.get(object) else {
            return &[];
        };
        let a = o.first_point as usize;
        self.points.get(a..a + o.point_count as usize).unwrap_or(&[])
    }

    /// Faces of one object; empty when the range is out of bounds.
    pub fn object_faces(&self, object: usize) -> &[Face] {
        let Some(o) = self.objects.get(object) else {
            return &[];
        };
        let a = o.first_face as usize;
        self.faces.get(a..a + o.face_count as usize).unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn disc(name: &str) -> Option<Vec<u8>> {
        let p = concat!(env!("CARGO_MANIFEST_DIR"), "/../../disc/extracted/PUNKY/");
        std::fs::read(format!("{p}{name}")).ok()
    }

    #[test]
    fn synthetic() {
        let mut d = Vec::new();
        for v in [1u32, 3, 1, 0, 0, 0, 3, 1] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        for v in [0f32, 0., 0., 1., 0., 0., 0., -1., 0.] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        for v in [4i16, 0, 0, 0, 1, 2, 0, 31, 0, 0, 0, 31] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        let m = Mesh3d::parse(&d).unwrap();
        assert_eq!(m.object_points(0)[2], [0., -1., 0.]);
        let f = m.object_faces(0)[0];
        assert_eq!((f.texture, f.points, f.u, f.v), (4, [0, 1, 2], [0, 31, 0], [0, 0, 31]));
        assert!(Mesh3d::parse(&d[..d.len() - 1]).is_err());
        assert!(m.object_points(1).is_empty());
    }

    #[test]
    fn disc_meshes() {
        for n in 1..=5 {
            let Some(d) = disc(&format!("OBJETS_{n}.3D")) else {
                eprintln!("disc files absent, skipping");
                return;
            };
            let m = Mesh3d::parse(&d).unwrap();
            // The layout accounts for the whole file.
            assert_eq!(d.len(), 16 + m.objects.len() * 16 + m.points.len() * 12 + m.faces.len() * 24);
            assert_eq!(m.header_unused, 0);
            // Objects tile the point and face arrays without gaps.
            let (mut p, mut f) = (0u32, 0u32);
            for (i, o) in m.objects.iter().enumerate() {
                assert_eq!((o.first_point, o.first_face), (p, f));
                p += o.point_count;
                f += o.face_count;
                for face in m.object_faces(i) {
                    assert_eq!(face.point_base as u32, o.first_point);
                    assert_eq!(face.unused, 0);
                    assert!(face.points.iter().all(|&k| k >= 0 && (k as u32) < o.point_count));
                    assert!(face.texture >= 0);
                }
            }
            assert_eq!((p as usize, f as usize), (m.points.len(), m.faces.len()));
            assert!(m.points.iter().flatten().all(|c| c.is_finite()));
        }
    }
}
