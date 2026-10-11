//! The original/enhanced switch.
//!
//! Every deliberate deviation from the DOS original is a named field here and is off
//! in [`Enhancements::original`]. Code never changes original behaviour unconditionally.
//! Add fields as needed; document each with the original behaviour it replaces and the
//! spec section (`docs/re/...`) that describes it.

/// Length of the truce of [`Enhancements::safe_spawn`] in 70 Hz ticks (5 seconds, of
/// which the fade-in takes about one).
pub const SPAWN_GRACE_TICKS: i32 = 350;

/// Half the side of the ground window with [`Enhancements::far_view`], in cells.
pub const FAR_VIEW_CELLS: i32 = 60;

#[derive(Clone, Debug, PartialEq)]
pub struct Enhancements {
    /// Present the whole island view. The original passes `width - 1` / `height - 1` to its
    /// blitters, so the last column and row of the view stay border colour
    /// (`docs/re/engine/renderer.md`, "Video modes and presentation").
    pub present_full_view: bool,
    /// Free a player shot slot after its last explosion frame. The original leaves the
    /// slot at state -1, which the monster hit test treats as a live shot, so a spent
    /// explosion keeps damaging monsters near it (`docs/re/game/player.md` 5).
    pub fix_spent_shot_slots: bool,
    /// Damage a monster by the weapon that fired the shot. The original uses the weapon
    /// selected at the moment of impact (`docs/re/game/entities.md` 3.3).
    pub monster_damage_uses_shot_type: bool,
    /// Do not let the HUD take a life. The original HUD deducts one when its displayed
    /// energy goes below 0, on top of the death sequence (`docs/re/game/player.md` 4).
    pub fix_hud_double_life_loss: bool,
    /// Make the fifth teleporter pad of islands 3 and 4 work. The original renames it
    /// to the previous pad's active id, so it sends the duck to that pad's destination
    /// (`docs/re/game/items-and-scripts.md` 6, 7).
    pub fix_fifth_teleporter: bool,
    /// Monsters patrol their TRJ path around the spawn point. The original computes
    /// later waypoints around the world position and compares against a signed step,
    /// so path monsters drift off in a straight line (`docs/re/game/entities.md` 3.6).
    pub fix_monster_path_drift: bool,
    /// Finishing island 1 leads to island 2. The original rewrites the exit to the
    /// ending at `0x124ea` (`docs/re/game/overview.md`).
    pub island1_leads_to_island2: bool,
    /// Keep the compass over RESUME GAME, CONTINUE and LOAD GAME. The original asset
    /// function clears it on every entry into the frame loop
    /// (`docs/re/game/save-and-settings.md` 2).
    pub keep_compass_on_reentry: bool,
    /// Limit `item_rename` / `item_remove_all` to the 30 item slots. The original
    /// loops to the bonus counter and patches unrelated globals that equal the id
    /// (`docs/re/game/items-and-scripts.md` 2).
    pub fix_item_scan_overflow: bool,
    /// Give all 39 cluster explosions of weapon 7 the cluster type. The original
    /// writes the type one slot late, leaving the last one with its old type
    /// (`docs/re/game/player.md` 5).
    pub fix_cluster_last_slot_type: bool,
    /// Append pickups after the last used slot. The original appends with
    /// `bonus_set(bonus_last++, ..)`, which overwrites the last `ILE0n.TRJ` record and
    /// leaves a zeroed trailing slot (an energy pill at the world origin)
    /// (`docs/re/game/entities.md` 4).
    pub fix_bonus_append_off_by_one: bool,
    /// Modern controls on the island: mouse turns the duck (speed set by MOUSE SPEED in
    /// the controller options), A/D strafe without a modifier, the left mouse button
    /// fires, number keys select a weapon directly (quest pictures move from `1`/`2` to
    /// F3/F4), E toggles the jetpack like Enter. The original turns with left/right,
    /// strafes with Ctrl + left/right, fires with Space, cycles weapons with Backspace
    /// and pauses on the mouse button (`docs/re/engine/input.md`).
    pub modern_controls: bool,
    /// Strafe over shallow water too, wading or flying. The original ignores the strafe
    /// keys while the duck is over a zone 1 cell (`player_move`, `0x10860`;
    /// `docs/re/game/player.md` 3).
    pub strafe_in_water: bool,
    /// See four times as far: the ground window is [`FAR_VIEW_CELLS`] cells to each
    /// side instead of 15, the fog moves out with it, and scenery and quest items are
    /// drawn up to that distance instead of 4500 units. Monsters still wake up at the
    /// original 6000 units (`docs/re/engine/renderer.md`, `docs/re/game/entities.md`).
    pub far_view: bool,
    /// Monsters ignore the duck for the first seconds after it appears: at the start of
    /// an island, after losing a life and after CONTINUE ([`SPAWN_GRACE_TICKS`]; over
    /// at once when the duck fires). They neither notice it nor shoot, and those that
    /// were chasing it turn back. In the original the monsters next to the start point
    /// open fire while the picture is still fading in (`docs/re/game/entities.md` 3).
    pub safe_spawn: bool,
    /// Draw the island view on every frame the frontend shows, between the last two
    /// simulation frames, with mouse turning applied at once. The original draws one
    /// picture per simulation frame (`docs/re/engine/timing.md`). The simulation and
    /// its timing are unchanged.
    pub smooth_view: bool,
    /// Mouse in the menus and 2D screens: pointing selects an entry, the left button
    /// chooses it (on a slider: left half lowers, right half raises), the right button
    /// is Esc; a click dismisses pictures and dialogue; the shop's hand follows the
    /// pointer. The original reads the keyboard only (`docs/re/engine/menus.md`).
    pub menu_mouse: bool,
    /// Pictures, dialogue and the map stay up until Space, Enter, Tab or the left mouse
    /// button is pressed. In the original any game key ends them, movement keys
    /// included, so walking into something skips its text (`0x2a2f0`,
    /// `docs/re/engine/menus.md`).
    pub confirm_to_dismiss: bool,
    /// W/S/A/D act as the cursor keys and Space as Enter in the menus (not while
    /// naming a save; capitals and the spaces of the level-select phrase are still
    /// read as text). The original reads them as text only.
    pub menu_wasd_space: bool,
    /// Esc in the main menu returns to the game in progress. The original moves to the
    /// last entry and opens EXIT GAME ? (`menu_input`, `0x25c90`).
    pub menu_esc_resumes: bool,
    /// A small map of the explored surroundings in the island view. The original only
    /// has the full-screen map (`map_screen`, `0x280f0`).
    pub minimap: bool,
}

impl Enhancements {
    /// Behaviour identical to the DOS original, bugs included.
    pub fn original() -> Self {
        Self {
            present_full_view: false,
            fix_spent_shot_slots: false,
            monster_damage_uses_shot_type: false,
            fix_hud_double_life_loss: false,
            fix_fifth_teleporter: false,
            fix_monster_path_drift: false,
            island1_leads_to_island2: false,
            keep_compass_on_reentry: false,
            fix_item_scan_overflow: false,
            fix_cluster_last_slot_type: false,
            fix_bonus_append_off_by_one: false,
            modern_controls: false,
            strafe_in_water: false,
            far_view: false,
            safe_spawn: false,
            smooth_view: false,
            menu_mouse: false,
            confirm_to_dismiss: false,
            menu_wasd_space: false,
            menu_esc_resumes: false,
            minimap: false,
        }
    }

    /// All fixes and improvements on.
    pub fn enhanced() -> Self {
        Self {
            present_full_view: true,
            fix_spent_shot_slots: true,
            monster_damage_uses_shot_type: true,
            fix_hud_double_life_loss: true,
            fix_fifth_teleporter: true,
            fix_monster_path_drift: true,
            island1_leads_to_island2: true,
            keep_compass_on_reentry: true,
            fix_item_scan_overflow: true,
            fix_cluster_last_slot_type: true,
            fix_bonus_append_off_by_one: true,
            modern_controls: true,
            strafe_in_water: true,
            far_view: true,
            safe_spawn: true,
            smooth_view: true,
            menu_mouse: true,
            confirm_to_dismiss: true,
            menu_wasd_space: true,
            menu_esc_resumes: true,
            minimap: true,
        }
    }
}

/// Every flag as `(field name, group, one-line description)`, in the order the
/// frontends list them. No flag depends on another: any combination is valid.
pub const FLAGS: &[(&str, &str, &str)] = &[
    ("modern_controls", "Controls", "Mouse look, A/D strafing, click to fire, number keys for weapons, E for the jetpack"),
    ("strafe_in_water", "Controls", "Strafing also works over shallow water"),
    ("smooth_view", "Picture", "Smooth view, drawn on every screen refresh"),
    ("far_view", "Picture", "See four times as far"),
    ("minimap", "Picture", "Minimap in the corner of the view"),
    ("present_full_view", "Picture", "Show the view's last pixel row and column"),
    ("menu_mouse", "Menus", "Mouse in menus, shop, pictures and dialogue"),
    ("menu_wasd_space", "Menus", "W/S/A/D and Space work in menus"),
    ("confirm_to_dismiss", "Menus", "Pictures and dialogue wait for Space, Enter or a click, not any key"),
    ("menu_esc_resumes", "Menus", "Esc in the main menu returns to the game"),
    ("island1_leads_to_island2", "Game", "Island 1 leads on to island 2 instead of the ending"),
    ("keep_compass_on_reentry", "Game", "Keep the compass after loading or resuming"),
    ("safe_spawn", "Game", "Monsters leave you alone for a few seconds after you appear"),
    ("fix_hud_double_life_loss", "Bug fixes", "A death costs one life, never two"),
    ("fix_monster_path_drift", "Bug fixes", "Monsters keep to their patrol paths"),
    ("fix_fifth_teleporter", "Bug fixes", "The fifth teleporter pad on islands 3 and 4 works"),
    ("monster_damage_uses_shot_type", "Bug fixes", "Damage is that of the weapon that fired the shot"),
    ("fix_spent_shot_slots", "Bug fixes", "Spent shots stop hurting monsters"),
    ("fix_cluster_last_slot_type", "Bug fixes", "Cluster bomb: every explosion counts"),
    ("fix_bonus_append_off_by_one", "Bug fixes", "Dropped pickups no longer replace the last placed one"),
    ("fix_item_scan_overflow", "Bug fixes", "Quest item bookkeeping stays inside its table"),
];

impl Enhancements {
    fn flag_mut(&mut self, name: &str) -> Option<&mut bool> {
        Some(match name {
            "modern_controls" => &mut self.modern_controls,
            "strafe_in_water" => &mut self.strafe_in_water,
            "smooth_view" => &mut self.smooth_view,
            "far_view" => &mut self.far_view,
            "minimap" => &mut self.minimap,
            "present_full_view" => &mut self.present_full_view,
            "menu_mouse" => &mut self.menu_mouse,
            "menu_wasd_space" => &mut self.menu_wasd_space,
            "confirm_to_dismiss" => &mut self.confirm_to_dismiss,
            "menu_esc_resumes" => &mut self.menu_esc_resumes,
            "island1_leads_to_island2" => &mut self.island1_leads_to_island2,
            "keep_compass_on_reentry" => &mut self.keep_compass_on_reentry,
            "safe_spawn" => &mut self.safe_spawn,
            "fix_hud_double_life_loss" => &mut self.fix_hud_double_life_loss,
            "fix_monster_path_drift" => &mut self.fix_monster_path_drift,
            "fix_fifth_teleporter" => &mut self.fix_fifth_teleporter,
            "monster_damage_uses_shot_type" => &mut self.monster_damage_uses_shot_type,
            "fix_spent_shot_slots" => &mut self.fix_spent_shot_slots,
            "fix_cluster_last_slot_type" => &mut self.fix_cluster_last_slot_type,
            "fix_bonus_append_off_by_one" => &mut self.fix_bonus_append_off_by_one,
            "fix_item_scan_overflow" => &mut self.fix_item_scan_overflow,
            _ => return None,
        })
    }

    /// Switch the flag called `name` (a field name). `false` if there is none.
    pub fn set(&mut self, name: &str, on: bool) -> bool {
        self.flag_mut(name).map(|f| *f = on).is_some()
    }

    pub fn get(&self, name: &str) -> Option<bool> {
        self.clone().flag_mut(name).map(|f| *f)
    }

    /// All improvements except the named ones (unknown names are ignored).
    pub fn enhanced_without<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        let mut e = Self::enhanced();
        for n in names {
            e.set(n.trim(), false);
        }
        e
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flag_table_covers_every_field() {
        let mut e = Enhancements::enhanced();
        for (name, _, _) in FLAGS {
            assert!(e.set(name, false), "{name}");
        }
        assert_eq!(e, Enhancements::original());
        assert_eq!(Enhancements::enhanced_without(["minimap", "nonsense"]).get("minimap"), Some(false));
    }
}

impl Default for Enhancements {
    fn default() -> Self {
        Self::original()
    }
}
