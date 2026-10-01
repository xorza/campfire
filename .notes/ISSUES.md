# Issues
- The 3v3 creep AI chooses only enemy creeps and avatars, never a structure, so creeps never attack a tower, an inhibitor or a core.
- A modifier's `states` load and do nothing: no unit is ever stunned, rooted, silenced, disarmed, airborne, stealthed, untargetable or slow immune, though the reference heroes name some of them.
- The load check accepts every `ctx` call and value design 08 lists, but 20 have no implementation and fail when a script uses them: `range`, `charge`, `origin`, `chance`, `pick`, `stun`, `slow`, `knock_up`, `knock_back`, `dash`, `teleport`, `projectile`, `area`, `reveal`, `reduce_cooldown`, `reduce_cooldowns`, `add_charge`, `add_xp`, `order_move`, `order_reset`.
- The unit handle has no `spawn_pos`, and there is no projectile or area handle; the 3v3 camp AI fails on `unit.spawn_pos`.
- Seven hooks the load check accepts never run: `on_projectile_hit`, `on_projectile_end`, `on_area_trigger`, `on_dash_end`, `on_channel_tick`, `on_player_join`, `on_player_leave`.
- A unit type's `true_sight` loads and does nothing.
