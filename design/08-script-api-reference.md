# Campfire — Script API reference

Generated from the script API's registry ([One source](08-script-api.md#one-source)); do not edit it. A test fails when it differs from what the registry writes; run that test with `CAMPFIRE_BLESS=1` to write it again. A name that runs is bound by the code that runs it; a planned one is one design 08 gives that the release does not run yet. The rules of the API are [design 08](08-script-api.md).

## `ctx`

| Name | Form | Roles | Capability | Status | What it is |
| --- | --- | --- | --- | --- | --- |
| `add_charge` | `(unit, id)` | every role | abilities | planned | gives `unit`'s ability `id` a charge |
| `add_modifier` | `(unit, id) or (unit, id, duration_ms)` | every role | stats | runs | applies the modifier `id` of the script's package to `unit` from the acting unit, and returns its handle |
| `add_resource` | `(player, name, amount)` | every role | core | runs | adds `amount` of the player resource `name` to `player` |
| `add_xp` | `(avatar, amount)` | every role | stats | planned | gives `avatar` experience |
| `area` | `(pos)` | ability | areas | planned | the ability's area at `pos` |
| `attack_hit` | `(target)` | ability, modifier, AI | combat | runs | an extra attack of the acting unit on `target`: no crit, and no `on_attack` |
| `avatar_available` | `(player, id)` | mode | core | runs | whether `player` may choose the avatar `id`: the mode depends on it, and no other player chose it |
| `avatars` | `() or (team)` | every role | core | runs | the avatars, living or dead, of every team or of `team`, by stable id |
| `chance` | `(p)` | every role | core | planned | true with probability `p`, from the secret stream |
| `charge` | read | ability | abilities | planned | how long a charged cast was held, from 0 to 1 |
| `choose_avatar` | `(player, id)` | mode | core | runs | chooses the avatar `id` for `player` |
| `choose_loadout` | `(player, ids)` | mode | core | runs | chooses `ids`, each a loadout entry the mode depends on, none twice, for `player` |
| `damage` | `(target, amount, kind)` | every role | combat | runs | deals `amount` of `kind`, one of the mode's `damage_kinds`, to `target` |
| `dash` | `(unit, to, speed)` | every role | navigation | planned | moves `unit` to `to` at `speed` |
| `end` | `(team) or (())` | mode | core | runs | ends the match, once: `team` wins, `()` is a draw |
| `enemy_team` | `(team)` | every role | core | runs | the one team that is `team`'s enemy, in a mode of two playing teams |
| `find` | `(of, pos, radius, filter)` | every role | core | runs | the living units within `radius` of `pos` that `filter` selects for `of`, seen or not, by stable id |
| `find_visible` | `(of, pos, radius, filter)` | every role | vision | runs | as `find`, of the units `of`'s team sees |
| `heal` | `(unit, amount)` | every role | combat | runs | heals `unit`, scaled by its `healing_received_pct` |
| `knock_back` | `(unit, from, distance, ms)` | every role | stats | planned | pushes `unit` away from `from` |
| `knock_up` | `(unit, ms)` | every role | stats | planned | the engine's knock up, from the acting unit |
| `learn` | `(avatar, slot)` | mode | abilities | runs | the ability in `slot` a rank more, up to its last |
| `map` | read | every role | navigation | runs | the map's paths and neutral spawns |
| `nearest_visible` | `(of, radius, filter)` | every role | vision | runs | the nearest living unit within `radius` of `of` that `filter` selects and `of`'s team sees, `()` with none |
| `order_attack` | `(unit, target)` | AI | orders | runs | `unit`, which has an attack, attacks `target`, a living enemy |
| `order_follow_path` | `(unit)` | AI | orders | runs | `unit` drops its target and walks its path again |
| `order_move` | `(unit, pos)` | AI | orders | planned | `unit` walks to `pos` |
| `order_reset` | `(unit)` | AI | orders | planned | `unit` walks home, heals, and drops its target |
| `origin` | read | ability | abilities | planned | where the cast comes from |
| `p` | read | every role | core | runs | the params: an ability's at its rank, a modifier's then its ability's, or the mode's |
| `pick` | `(list)` | every role | core | planned | an entry of `list`, from the secret stream |
| `players` | read | mode | core | runs | how many players the session has |
| `projectile` | `(from, to) or (from, to, overrides)` | ability | projectiles | planned | the ability's projectile, flying a line, homing on a unit or flying to a position |
| `range` | read | ability | abilities | planned | the ability's range at its rank |
| `reduce_cooldown` | `(unit, id, ms)` | every role | abilities | planned | takes `ms` off the cooldown of `unit`'s ability `id` |
| `reduce_cooldowns` | `(unit, fraction)` | every role | abilities | planned | takes `fraction` off the cooldowns of `unit`'s basic abilities |
| `remove` | `(handle)` | every role | stats | runs | ends the modifier, projectile or area at once |
| `respawn` | `(unit, ms)` | mode | combat | runs | brings back `unit`, dead and of a type that stays, `ms` from the call |
| `restore` | `(unit, amount)` | every role | combat | runs | gives `unit` back `amount` of its resource |
| `reveal` | `(pos, radius, ms)` | every role | vision | planned | shows the source's team what is within `radius` of `pos` |
| `slow` | `(unit, fraction, ms)` | every role | stats | planned | the engine's slow, from the acting unit |
| `spawn_avatars` | `()` | mode | core | runs | spawns each chosen avatar not yet spawned, in slot order, at its team's spawn |
| `spawn_group` | `(team, path, types)` | mode | core | runs | spawns `types` in order at `team`'s end of `path`, walking it |
| `spawn_unit` | `(type, team, pos)` | mode | core | runs | spawns a unit of `type` on `team` at `pos`, within the map's bounds |
| `state` | read | mode | core | runs | the mode's state fields, by name, to read and write |
| `stun` | `(unit, ms)` | every role | stats | planned | the engine's stun, from the acting unit |
| `teams` | read | every role | core | runs | the playing teams' names |
| `teleport` | `(unit, pos)` | every role | navigation | planned | puts `unit` at `pos` |
| `timer` | `(name, ms, repeat, data)` | mode | core | runs | calls `on_timer` `ms` from the call, rounded up to whole ticks, at least one |
| `units_tagged` | `(tag)` | every role | core | runs | the units of a tag, living or dead, by stable id |

## Unit

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `!=` | operator | core | runs | whether the two are two units |
| `==` | operator | core | runs | whether the two are one unit |
| `alive` | read | core | runs | whether it lives |
| `attack_range` | read | combat | runs | its attack's range |
| `can_see` | `(unit)` | vision | runs | whether its team sees the other unit |
| `has_modifier` | `(id)` | stats | runs | whether it carries the modifier of the script's package |
| `has_tag` | `(tag)` | core | runs | whether its unit type has the tag |
| `health` | read | combat | runs | its health |
| `is_avatar` | read | core | runs | whether it is an avatar |
| `is_enemy_of` | `(unit)` | core | runs | whether the two are of enemy teams |
| `level` | read | stats | runs | its level |
| `max_health` | read | combat | runs | its health's maximum |
| `owner` | read | core | runs | its player's slot, `()` with none |
| `params` | read | core | runs | its unit type's params, unresolved |
| `path` | read | core | runs | the name of the path it walks, `()` with none |
| `pos` | read | core | runs | where it stands |
| `radius` | read | core | runs | its body's radius, 0 with no body |
| `recent_attackers` | `(ms)` | combat | runs | the living units that struck it within the last `ms`, rounded up to whole ticks |
| `spawn_pos` | read | core | planned | where it spawned |
| `stat` | `(name)` | stats | runs | its value of a stat the mode declares |
| `target` | read | core | runs | its attack's target, `()` with none |
| `team` | read | core | runs | its team's name |
| `unit_type` | read | core | runs | its unit type's name |

## Modifier `m`

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `carrier` | read | stats | runs | the unit that carries it |
| `source` | read | stats | runs | the unit that applied it, `()` when gone or none |
| `stacks` | written and read | stats | runs | its stacks, which a call may write and read back |
| `state` | read | stats | runs | its script state, by name, which a call may write and read back |

## Projectile

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `distance` | read | projectiles | planned | how far it flew |
| `pos` | read | projectiles | planned | where it flies |
| `source` | read | projectiles | planned | the unit that launched it |
| `state` | read | projectiles | planned | its script state, which a call may write |

## Area

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `pos` | read | areas | planned | where it lies |
| `source` | read | areas | planned | the unit that made it |

## Damage `d`

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `ability` | read | combat | runs | the ability that dealt it, `""` when none |
| `amount` | read | combat | runs | raw in `calc_damage`, final in a hook |
| `attack` | read | combat | runs | whether an attack dealt it |
| `crit` | read | combat | runs | whether its attack crit |
| `extra` | read | combat | runs | whether `ctx.attack_hit` dealt it |
| `kind` | read | combat | runs | one of the mode's `damage_kinds` |
| `source` | read | combat | runs | the unit that dealt it, `()` when gone or none |
| `target` | read | combat | runs | the unit it is dealt to |

## Position

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `direction_to` | `(pos)` | core | planned | the unit vector towards `pos` |
| `distance_to` | `(pos)` | core | runs | the distance to `pos` |
| `within` | `(pos, radius)` | core | runs | whether `pos` is within `radius` on the ground plane, exactly: the test for reach |

## Vector

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `*` | operator | core | planned | the vector scaled by a number |
| `+` | operator | core | planned | the sum of two vectors |
| `-` | operator | core | planned | the difference of two vectors |
| `rotated_deg` | `(degrees)` | core | planned | the vector turned by `degrees` |

## Map, `ctx.map`

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `neutral_spawns` | read | navigation | runs | the neutral spawns |
| `paths` | read | navigation | runs | the paths' names |

## Neutral spawn, of `ctx.map.neutral_spawns`

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `pos` | read | navigation | runs | where it spawns |
| `unit_type` | read | navigation | runs | the unit type it spawns |

## Hooks

| Hook | Role | Capability | Status |
| --- | --- | --- | --- |
| `on_cast(ctx, caster, target)` | ability | abilities | runs |
| `on_channel_tick(ctx, caster)` | ability | abilities | planned |
| `on_dash_end(ctx, unit, target)` | ability | abilities | planned |
| `on_projectile_hit(ctx, proj, target)` | ability | projectiles | planned |
| `on_projectile_end(ctx, proj)` | ability | projectiles | planned |
| `on_area_trigger(ctx, area, units)` | ability | areas | planned |
| `on_interval(ctx, m)` | modifier | stats | runs |
| `on_attack(ctx, m, target)` | modifier | combat | runs |
| `on_attack_hit(ctx, m, d)` | modifier | combat | runs |
| `on_damage_taken(ctx, m, d)` | modifier | combat | runs |
| `on_kill(ctx, m, victim)` | modifier | combat | runs |
| `on_takedown(ctx, m, victim)` | modifier | combat | runs |
| `on_match_start(ctx)` | mode | core | runs |
| `on_mode_input(ctx, player, name, value)` | mode | core | runs |
| `on_timer(ctx, name, data)` | mode | core | runs |
| `on_player_join(ctx, player)` | mode | core | planned |
| `on_player_leave(ctx, player)` | mode | core | planned |
| `on_unit_died(ctx, unit, killer, assisters)` | mode | combat | runs |
| `calc_damage(ctx, d)` | mode | combat | runs |
| `think(ctx, unit)` | AI | orders | runs |

## States

| State | Status |
| --- | --- |
| `stunned` | runs |
| `rooted` | runs |
| `silenced` | runs |
| `disarmed` | runs |
| `airborne` | runs |
| `stealthed` | runs |
| `untargetable` | runs |
| `slow_immune` | runs |
| `invulnerable` | runs |
| `true_sight` | runs |

## Data fields

### `data/mode.toml`

| Field | Status |
| --- | --- |
| `script` | runs |
| `assist_window_ms` | runs |
| `inputs` | runs |
| `state` | runs |
| `params` | runs |
| `modifiers` | runs |
| `damage_kinds` | runs |
| `attack_kind` | runs |
| `stats` | runs |
| `resources` | runs |
| `state_version` | planned |

### An ability, `[abilities.<id>]`

| Field | Status |
| --- | --- |
| `script` | runs |
| `targeting` | runs |
| `range` | runs |
| `cooldown_ms` | runs |
| `cost` | runs |
| `cast_time_ms` | runs |
| `passive_modifier` | runs |
| `passive_while_ready` | runs |
| `params` | runs |
| `clamp_to_range` | planned |
| `toggle` | planned |
| `channel` | planned |
| `hold` | planned |
| `charges` | planned |
| `charge` | planned |
| `projectile` | planned |
| `area` | planned |
| `projectile_state` | planned |

### A modifier, `[modifiers.<id>]`

| Field | Status |
| --- | --- |
| `script` | runs |
| `duration_ms` | runs |
| `interval_ms` | runs |
| `stacks_expire_ms` | runs |
| `reapply` | runs |
| `max_stacks` | runs |
| `stats` | runs |
| `shield` | runs |
| `aura` | runs |
| `params` | runs |
| `state` | runs |
| `states` | runs |

### A modifier's `aura`

| Field | Status |
| --- | --- |
| `radius` | runs |
| `affects` | runs |
| `modifier` | runs |

### A unit type's `combat`

| Field | Status |
| --- | --- |
| `attack` | runs |
| `on_death` | runs |

### A unit type's `combat.attack`

| Field | Status |
| --- | --- |
| `range` | runs |
| `windup_ms` | runs |
| `projectile_speed` | runs |

### A unit type's `vision`

| Field | Status |
| --- | --- |
| `sight_range` | runs |
| `true_sight` | runs |

### A unit type's `collision`

| Field | Status |
| --- | --- |
| `radius` | runs |

### A unit type's `orders`

| Field | Status |
| --- | --- |
| `ai` | runs |
| `think_ms` | runs |
