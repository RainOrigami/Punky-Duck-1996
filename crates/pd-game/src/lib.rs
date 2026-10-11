//! Punky Duck game logic, ported from `P.EXE` (specs in `docs/re/`).
//!
//! - [`render`]: software renderer for the island view (terrain, 3D objects, world sprites).
//!   Draws a [`render::Scene`] description; knows nothing about game rules.
//! - [`sim`]: gameplay simulation (terrain queries, player, weapons, monsters, pickups).
//!   Pure state stepped by ticks; draws nothing.
//! - [`app`]: program flow implementing `pd_engine::App` (menus, HUD, save/load, settings,
//!   pictures, movies) and the glue between `sim` and `render`.
//! - [`settings`]: the original/enhanced switch.

pub mod app;
pub mod render;
pub mod settings;
pub mod sim;
