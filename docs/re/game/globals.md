# Gameplay globals (name ↔ address)

Names used in `docs/re/game/*.md`. Addresses as in `P.elf`. i32 unless noted. For engine
globals see `docs/re/engine/` and `functions.md` (those names win on conflict).

| Addr | Name | Notes |
|---|---|---|
| `0x517f88` | `island` | 1..5 |
| `0x517f04` | `next_island` | 0 stay; 2..5 next; 6 ending |
| `0x41624` | `difficulty` | 0..2 |
| `0x416e4` | `language` | 0..4 |
| `0x4000c` | `game_speed` | −6..6 |
| `0x41628` | `attract_off` | 0 = demo playback mode |
| `0x41600` | `demo_record` (u8) | |
| `0x416e0` | `level_select` (u8) | cheat flag |
| `0x4191a` | `cheat_flags` (u16) | bit 15 enabled; 0x1 lives, 0x2 energy, 0x4 ammo, 0x8 fuel, 0x10 power, 0x80 compass, 0x100 money |
| `0x41b02` | `tick_counter` | +1 per 70 Hz interrupt |
| `0x517f70` | `ticks` | steps this frame − 1 |
| `0x50d5c4..d3` | input block (16 × u8) | `player.md` 7 |
| `0x50d5e0/e1/e2/e3` | menu result: resume / exit / new game (or continue) / game in progress (u8) | |
| `0x517edc`, `0x517ee4`, `0x517ed8` | `px`, `pz`, `py` | negated X/Z |
| `0x517f00`, `0x517f14` | `tx`, `tz` | |
| `0x517f9c`, `0x517f98` | `rx`, `rz` | |
| `0x517fa0` | `angle` (f32) | |
| `0x517f84` | `slide` | |
| `0x40018`, `0x4001c` | `speed` 34, `fly_speed` 60 | constants |
| `0x517f80` | `zone` | |
| `0x517ee8`, `0x517f24` | `flying`, `fuel` | |
| `0x517eec` | `hurt` | |
| `0x517f20`, `0x517f0c`, `0x517f18`, `0x517ed4` | `energy`, `lives`, `continues`, `money` | |
| `0x517ec0`, `0x517ed0` | `dead`, `dead_wait` | |
| `0x517eb8`, `0x517eb4` | `start_x`, `start_z` | |
| `0x517f08`, `0x517f60` | `swim_timer`, `drown` | |
| `0x517f68`, `0x517f54` | `walk`, `strafe_ctr` | |
| `0x517f64`, `0x517f7c`, `0x517f74`, `0x517f5c` | `last_dir`, `move_dir`, `strafe`, `moved` | |
| `0x517eb0` | `bump` | |
| `0x517f1c` | `shooting` | |
| `0x517ee0`, `0x517f10`, `0x517ef8` | `weapon`, `power`, `power_shots` | |
| `0x517d80`, `0x517970` | `ammo[10]`, `owned[10]` | |
| `0x419ac`, `0x40028` | `fire_ctr`, `last_weapon` | |
| `0x419b0` | `cooldown[10][3]` | |
| `0x517ea4` | `compass` | |
| `0x517ea8` | `next_shot` | |
| `0x50dab0`, `0x50e280`, `0x50de98`, `0x50e668`, `0x517588`, `0x516db8`, `0x517998` | player shot arrays `state, x, y, z, vx, vz, type` [250] | |
| `0x50c7a0`, `0x50c79c`, `0x50c794` | `autoaim_found`, `autoaim_angle` (f32), `autoaim_best` (f32) | |
| `0x4f8944` | `monster[210]`, 0x184 bytes each | `entities.md` |
| `0x4f6144` | MONSTERn.TRJ buffer | |
| `0x414a8`, `0x414b4`, `0x414b8`, `0x414bc` | `nmonsters`, records ptr, paths ptr, points ptr | |
| `0x414c0` | monster sprite tables [5][11] | |
| `0x4159b` | `class_of[type]` (u8, index 1..4) | |
| `0x415a0` | `boss_muzzle[island]` | |
| `0x415d4` | zone → monster flags (u8[32]) | |
| `0x517ec8` | `boss_state` | |
| `0x50c60c` | `boss_respawns` | |
| `0x50c798` | `alive_count` | |
| `0x50ea50` | bonus list [1600] × 20 bytes | |
| `0x517ec4` | `bonus_last` | |
| `0x5167c8` | `bonus_frames[100]` {first, last} | |
| `0x516ae8` | quest items [30] × 24 bytes | |
| `0x517ecc` | `item_last` | |
| `0x517f28`, `0x517eac`, `0x517f40` | `touch`, `touch_slot`, `busy` | |
| `0x517f2c`, `0x517f50` | `step`, `step_shown` | |
| `0x517e20`, `0x517ef0` | `inv[20]`, `ninv` | |
| `0x517f30`, `0x517f4c` | `model`, `model2` | |
| `0x517ef4` | `boss_warned` | |
| `0x517e9c` | island 1 `site_seen` | |
| `0x517e90`, `0x517e70` | island 2 flags | |
| `0x517e78` | island 5 `outfit_on` | |
| `0x517e80`, `0x517e88`, `0x517e84`, `0x517e7c` | island 5 vessel destroyed flags | |
| `0x516750`, `0x50da38` | teleporter marker x/z [10] | |
| `0x50d948`, `0x50d9c0` | hint marker x/z [20] | |
| `0x2456a8` | `explored` map (0x10000) | |
| `0x22cdc8` | zone (attribute) map [256][256] | |
| `0x1f2668` | height map i16 [256][256] | |
| `0x4f0ec0` | `cell` = 300 | |
| `0x415fc`, `0x415f8` | HUD displayed energy / money | |

Function names used: `player_move` `0x10860`, `player_shots` `0x103b0`, `shot_explosions`
`0x10290`, `player_draw` `0x11400`, `view_keys` `0x11960`, `monsters_draw` `0x11be0`,
`monsters_update` `0x21af0`, `idle_move` `0x22c60`, `monster_animate` `0x23860`,
`monster_face` `0x236a0`, `monster_hit` `0x23be0`, `monster_shots` `0x23260`,
`monster_contact` `0x23ec0`, `monster_zone` `0x218b0`, `monsters_init` `0x24110`,
`island_reset` `0x215b0`+, `mark` `0x27de0`, `hint` `0x27e50`, `item_add` `0x27ec0`,
`bonus_set` `0x27f30`, `bonus_load` `0x27fd0`, `map_screen` `0x280f0`, `item_remove_all`
`0x28670`, `inv_remove` `0x286b0`, `picture` `0x28750`, `picture_fade` `0x28870`,
`picture_timed` `0x28990`, `item_rename` `0x28b70`, `bonus_random_money` `0x28bc0`,
`zone_fill` `0x28ce0`, `inv_has` `0x28d20`, `tick_loop` `0x28d50`, `say` `0x28e00`,
`text_on_picture` `0x290e0`, `new_game_reset` `0x292d0`, `state_load` `0x293c0`,
`state_save` `0x29870`, `apply_pickup` `0x29d20`, `frame_common` `0x2a260`, `shop_check`
`0x2a450`, `hud` `0x246b0`, `terrain_height` `0x13010`, `zone_at` `0x151c0`.
