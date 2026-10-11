//! Parsers and decoders for the original Punky Duck data formats.
//!
//! Pure functions over `&[u8]`; no filesystem, clock or thread access.
//! Specs live in `docs/re/formats/`.

pub mod bnk;
pub mod col;
pub mod disc;
pub mod flc;
pub mod mesh3d;
pub mod objmap;
pub mod pcm;
pub mod ron;
pub mod spt;
pub mod stp;
pub mod trj;
pub mod txt;
pub mod uni;
