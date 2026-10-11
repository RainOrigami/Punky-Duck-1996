//! Software renderer of the island view, ported from `P.EXE`
//! (`docs/re/engine/renderer.md`).
//!
//! The renderer knows no game rules. Its whole input is a [`Scene`]: camera,
//! view mode, the loaded island data ([`IslandAssets`]), 3D object instances,
//! world sprites and the few per-frame counters the original keeps in globals
//! ([`ViewState`]). [`Renderer::render`] draws the scene into its own back
//! buffer (the original's `0x4a5ae0`, pitch 640), the caller may then draw the
//! HUD into [`Renderer::back_mut`], and [`Renderer::present`] copies the view
//! into the 640x480 [`pd_engine::Framebuffer`] the way `render_frame` does.
//!
//! ```ignore
//! let island = IslandAssets::from_bytes(uni, col, Some((objets_3d, objets_map)), Some(ciel_ron))?;
//! let mut renderer = Renderer::new();
//! let mut view = ViewState::default();
//! // per rendered frame:
//! view.begin_frame(player_py);                       // camera smoothing, water phase
//! let scene = Scene { island: &island, mode: ViewMode::High, interlace: false,
//!     camera: Camera { x: cam_x, z: cam_z, heading: angle }, view,
//!     objects: &objects, sprites: &sprites };
//! view.horizon = renderer.render(&scene).horizon;    // feeds next frame's sky
//! /* draw the HUD into renderer.back_mut() */
//! renderer.present(&mut fb, scene.mode, scene.interlace, &enhancements);
//! ```

mod raster;
mod renderer;

pub use raster::PITCH;
pub use renderer::{FrameInfo, Renderer, ground_window};

use pd_formats::col::Col;
use pd_formats::mesh3d::Mesh3d;
use pd_formats::objmap::ObjMap;
use pd_formats::ron;
use pd_formats::spt::{Sprite, SpriteSheet};
use pd_formats::uni::Uni;

/// World units per terrain cell (`0x4f0ec0`).
pub const CELL: i32 = 300;
/// `0x40034`: far limit of the ground and of world sprites, in depth units.
pub const FOG_FAR: i32 = 6000;
/// `0x40030`: depth range of the fog; it starts at `FOG_FAR - FOG_RANGE` = 4500.
pub const FOG_RANGE: i32 = 1500;
/// `0x4003c`: 3D object faces are drawn when nearer than `OBJECT_FAR - 1500`.
pub const OBJECT_FAR: i32 = 8000;
/// `world_sprite_add` keeps at most 99 sprites per frame (`0x138fc`).
pub const MAX_SPRITES: usize = 99;
/// Sprites and objects farther than this from the camera position in x or z
/// are dropped when queued (`0x1194`, `0x138f0` / `0x14050`).
pub const QUEUE_RANGE: i32 = 4500;
/// How far the view reaches. The original's values are compiled in
/// ([`Reach::ORIGINAL`]); `Enhancements::far_view` widens them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reach {
    /// Half the side of the ground vertex window in cells (`0x126e8`).
    pub half: i32,
    /// Far limit of ground and sprites in depth units (`0x40034`).
    pub fog_far: i32,
    /// Depth range of the fog before the far limit (`0x40030`).
    pub fog_range: i32,
    /// Sprites queued per frame at most (`0x138fc`).
    pub max_sprites: usize,
}

impl Reach {
    pub const ORIGINAL: Reach =
        Reach { half: 15, fog_far: FOG_FAR, fog_range: FOG_RANGE, max_sprites: MAX_SPRITES };

    /// A window of `half` cells to each side with the fog in the original's
    /// proportions. Depths are 16-bit in the renderer, so at most 90.
    pub fn cells(half: i32) -> Reach {
        let half = half.clamp(15, 90);
        Reach { half, fog_far: FOG_FAR * half / 15, fog_range: FOG_RANGE * half / 15, max_sprites: 2000 }
    }

    /// Sprites and objects farther than this from the camera in x or z are
    /// not queued (`0x1194`): the edge of the ground window.
    pub fn queue_range(&self) -> i32 {
        self.half * CELL
    }
}

/// Border colour the original fills the screen with at a mode switch.
pub const BORDER_COLOUR: u8 = 0xfe;
/// Colour of the haze band between sky picture and ground (`sky_draw`).
pub const HAZE_COLOUR: u8 = 0x40;

/// View mode, `0x4002c` (F1 cycles 0 → 1 → 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ViewMode {
    /// 0: 320x200 rendered, shown full screen in BIOS mode 13h.
    Low13h,
    /// 1: 320x200 rendered, shown at (160, 80) of the 640x480 screen.
    LowWindow,
    /// 2 (default): 640x400 rendered, shown at (0, 40).
    #[default]
    High,
}

impl ViewMode {
    /// Rendered size in pixels.
    pub fn size(self) -> (i32, i32) {
        match self {
            ViewMode::High => (640, 400),
            _ => (320, 200),
        }
    }

    /// Index into the fixed sprite size table `0x126f4` = `{0.5, 0.5, 1.0}`.
    pub fn index(self) -> usize {
        self as usize
    }
}

/// Camera inputs of `camera_update` (`0x2b1c0`).
///
/// `x`, `z` are the globals `0x517f8c` / `0x517f94`, i.e. the player's stored
/// position, which is the **negated** world position. `heading` is `0x517fa0`
/// in radians; the view looks along world `(+cos, +sin)`.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Camera {
    pub x: i32,
    pub z: i32,
    pub heading: f32,
}

/// Renderer globals that persist from frame to frame in the original.
/// Keep one per island visit and call [`ViewState::begin_frame`] once per
/// rendered frame, before building the [`Scene`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewState {
    /// `0x40074`: eye height in height units (terrain height byte x 3).
    pub eye_h: i32,
    /// `0x40078`: height of the look-at point.
    pub look_h: i32,
    /// `0x4f0e14`: water animation phase (already advanced for this frame).
    pub wave_phase: i32,
    /// `0x4f0e64`: screen row of the horizon used by the sky. Set by the
    /// previous frame's horizon marker sprite, see [`FrameInfo::horizon`].
    pub horizon: i32,
}

impl Default for ViewState {
    /// Values of a freshly started `P.EXE` after the first `view_mode_apply`.
    fn default() -> Self {
        Self { eye_h: 50, look_h: 0, wave_phase: 0, horizon: 200 }
    }
}

impl ViewState {
    /// Per-frame part of `camera_update` and `ground_build_vertices`:
    /// eye and look-at heights chase `altitude / 300 + 250` and `+ 100` by a
    /// fifth of the remaining distance (integer division, so they stop up to
    /// 4 short), and the water phase advances by 16.
    /// `altitude` is the player's `py` (`0x517ed8`).
    pub fn begin_frame(&mut self, altitude: i32) {
        let base = altitude / CELL;
        self.eye_h += (base + 250 - self.eye_h) / 5;
        self.look_h += (base + 100 - self.look_h) / 5;
        self.wave_phase = self.wave_phase.wrapping_add(16);
    }

    /// Runs [`ViewState::begin_frame`] until the heights stop changing
    /// (what a camera at rest looks like).
    pub fn settle(&mut self, altitude: i32) {
        loop {
            let before = (self.eye_h, self.look_h);
            let phase = self.wave_phase;
            self.begin_frame(altitude);
            self.wave_phase = phase;
            if before == (self.eye_h, self.look_h) {
                break;
            }
        }
    }
}

/// One slot of the 3D object table, the arguments of `object3d_place`
/// (`0x14050`). The slot number is the index in [`Scene::objects`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObjectInstance {
    /// `false` leaves the slot empty.
    pub active: bool,
    /// Index into `OBJETS_n.3D`'s object table.
    pub model: usize,
    /// World position as passed by the game code (true world coordinates;
    /// `y` in world units, divided by 300 by the renderer).
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// The two rotation angles the renderer uses (arguments 6 and 7).
    /// The third angle the game passes is stored but never applied.
    pub rot_a: f32,
    pub rot_b: f32,
}

/// Sprite corner reference, last argument of `world_sprite_add`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Anchor {
    /// 0: the SPT hot spot is the anchor.
    #[default]
    HotSpot,
    /// 1: the sprite centre.
    Centre,
    /// 2: bottom centre.
    BottomCentre,
}

/// `flags` bit: draw mirrored.
pub const SPRITE_MIRROR: u32 = 1;
/// `flags` bit: every opaque pixel becomes colour 0xFF (hit flash).
pub const SPRITE_FLASH: u32 = 8;

/// One queued billboard, the arguments of `world_sprite_add` (`0x138f0`).
#[derive(Clone, Copy)]
pub struct WorldSprite<'a> {
    pub bank: &'a SpriteBank,
    /// Sprite number in the bank.
    pub index: usize,
    /// [`SPRITE_MIRROR`], [`SPRITE_FLASH`].
    pub flags: u32,
    /// True world position. `y >= 0` is an absolute height in world units
    /// (divided by 300); `y < 0` places the sprite `-y / 300` height units
    /// above the ground.
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// In-plane rotation in radians.
    pub angle: f32,
    /// `> 0`: size factor relative to the projected size of 60 height units
    /// divided by 200. `-900 < scale < 0`: fixed size (0.5 in the 320 modes,
    /// 1.0 in 640). `<= -900`: horizon marker, not drawn; its screen row
    /// becomes [`FrameInfo::horizon`].
    pub scale: f32,
    pub anchor: Anchor,
}

/// A decoded `.SPT` sheet, kept by the caller for the lifetime of the scene.
pub struct SpriteBank {
    pub sprites: Vec<Sprite>,
}

impl SpriteBank {
    /// Decodes every sprite of an `.SPT` file. Sprites that fail to decode
    /// become empty (0x0) entries so that indices stay aligned.
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        let sheet = SpriteSheet::parse(data).map_err(|e| e.to_string())?;
        let sprites = (0..sheet.len())
            .map(|i| {
                sheet.decode(i).unwrap_or(Sprite {
                    width: 0,
                    height: 0,
                    hot_x: 0,
                    hot_y: 0,
                    pixels: Vec::new(),
                    mask: Vec::new(),
                })
            })
            .collect();
        Ok(Self { sprites })
    }
}

/// Everything `island_enter` loads for the renderer.
pub struct IslandAssets {
    /// Terrain. [`Uni::sheet`] may be modified between frames (animated
    /// tiles copy `Animaps.RON` frames into it).
    pub uni: Uni,
    /// 6-bit VGA palette of the island (`ILE_n.COL >> 2`).
    pub palette: [[u8; 3]; 256],
    /// `OBJETS_n.3D`, points as stored (the renderer applies the x2).
    pub meshes: Option<Mesh3d>,
    /// `OBJETS_n.MAP` texture page.
    pub objmap: Option<ObjMap>,
    /// `Ciel_n.RON` in a buffer of pitch 640 (`0x1980d0`).
    pub sky: Vec<u8>,
}

impl IslandAssets {
    /// Parses the island files. `objects` is `(OBJETS_n.3D, OBJETS_n.MAP)`,
    /// `ciel` the still XOR-ed `Ciel_n.RON` from `PICTURE.BNK`.
    pub fn from_bytes(
        uni: &[u8],
        col: &[u8],
        objects: Option<(&[u8], &[u8])>,
        ciel: Option<&[u8]>,
    ) -> Result<Self, String> {
        let uni = Uni::parse(uni).map_err(|e| e.to_string())?;
        let palette = Col::parse(col).map_err(|e| e.to_string())?.vga6();
        let (meshes, objmap) = match objects {
            Some((m, t)) => (
                Some(Mesh3d::parse(m).map_err(|e| e.to_string())?),
                Some(ObjMap::parse(t).map_err(|e| e.to_string())?),
            ),
            None => (None, None),
        };
        let mut sky = Vec::new();
        if let Some(c) = ciel {
            let pic = ron::decode(c).map_err(|e| e.to_string())?;
            let (w, h) = (pic.width as usize, pic.height as usize);
            sky = vec![0u8; PITCH * h];
            for y in 0..h {
                let n = w.min(PITCH);
                sky[y * PITCH..y * PITCH + n].copy_from_slice(&pic.pixels[y * w..y * w + n]);
            }
        }
        Ok(Self { uni, palette, meshes, objmap, sky })
    }

    /// The palette as 8-bit RGB for [`pd_engine::Framebuffer::palette`].
    pub fn palette_rgb8(&self) -> [[u8; 3]; 256] {
        self.palette.map(|c| c.map(ron::vga6_to_rgb8))
    }
}

/// The renderer's whole input for one frame.
pub struct Scene<'a> {
    pub island: &'a IslandAssets,
    pub mode: ViewMode,
    /// `0x40044` / `0x40054` (F2): render and present even rows only.
    /// Only effective in [`ViewMode::High`] for the sky and presentation;
    /// the triangle fillers honour it in every mode, as in the original.
    pub interlace: bool,
    pub camera: Camera,
    pub view: ViewState,
    /// Object slots in slot order.
    pub objects: &'a [ObjectInstance],
    /// Sprites in the order the game queues them. The renderer applies the
    /// queue's own limits ([`MAX_SPRITES`], [`QUEUE_RANGE`]).
    pub sprites: &'a [WorldSprite<'a>],
}
