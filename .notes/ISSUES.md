# Issues
- The 3v3 creep AI chooses only enemy creeps and avatars, never a structure, so creeps never attack a tower, an inhibitor or a core.
- The mode's load does not refuse a map whose structures leave a waypoint of a path unreachable from the one before it for the widest walker.
- A modifier's `states` load and do nothing: no unit is ever stunned, rooted, silenced, disarmed, airborne, stealthed, untargetable or slow immune, though the reference heroes name some of them.
- The load check accepts every `ctx` call and value design 08 lists, but 25 have no implementation and fail when a script uses them: `range`, `charge`, `origin`, `chance`, `pick`, `heal`, `restore`, `attack_hit`, `add_modifier`, `remove`, `stun`, `slow`, `knock_up`, `knock_back`, `dash`, `teleport`, `projectile`, `area`, `reveal`, `reduce_cooldown`, `reduce_cooldowns`, `add_charge`, `add_xp`, `order_move`, `order_reset`.
- The unit handle has no `level`, `health`, `max_health`, `stat(name)`, `spawn_pos` or `has_modifier`, and there is no modifier, projectile, area or damage handle; the 3v3 camp AI fails on `unit.spawn_pos`.
- Fourteen hooks the load check accepts never run: `calc_damage`, `on_attack`, `on_attack_hit`, `on_damage_taken`, `on_kill`, `on_takedown`, `on_interval`, `on_projectile_hit`, `on_projectile_end`, `on_area_trigger`, `on_dash_end`, `on_channel_tick`, `on_player_join`, `on_player_leave`; the 3v3 mode's armor and magic resistance in `calc_damage` never apply.
- A unit type's `true_sight` loads and does nothing.
