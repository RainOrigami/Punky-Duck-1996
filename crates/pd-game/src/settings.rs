//! The original/enhanced switch.
//!
//! Every deliberate deviation from the DOS original is a named field here and is off
//! in [`Enhancements::original`]. Code never changes original behaviour unconditionally.
//! Add fields as needed; document each with the original behaviour it replaces and the
//! spec section (`docs/re/...`) that describes it.

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
    /// Modern controls on the island: mouse turns the duck, A/D strafe without a
    /// modifier, the left mouse button fires, number keys select a weapon directly
    /// (quest pictures move from `1`/`2` to F3/F4). The original turns with left/right,
    /// strafes with Ctrl + left/right, fires with Space, cycles weapons with Backspace
    /// and pauses on the mouse button (`docs/re/engine/input.md`).
    pub modern_controls: bool,
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
        }
    }
}

impl Default for Enhancements {
    fn default() -> Self {
        Self::original()
    }
}
