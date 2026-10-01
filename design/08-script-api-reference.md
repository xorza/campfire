# Campfire — Script API reference

Generated from the script API's registry ([One source](08-script-api.md#one-source)); do not edit it. A test fails when it differs from what the registry writes; run that test with `CAMPFIRE_BLESS=1` to write it again. A name that runs is bound by the code that runs it; a planned one is one design 08 gives that the release does not run yet. The rules of the API are [design 08](08-script-api.md).

## `ctx`

| Name | Form | Roles | Capability | Status | What it is |
| --- | --- | --- | --- | --- | --- |
| `add_charge` | `(unit, id)` | every role | abilities | planned | gives `unit`'s ability `id` a charge |
| `add_modifier` | `(unit, id) or (unit, id, duration_ms)` | every role | stats | runs | applies the modifier `id` of the script's package to `unit` from the acting unit, and returns its handle |
| `add_player_modifier` | `(player, id)` | every role | stats | runs | gives `player` the modifier `id` of the script's package, which every living unit it owns that the modifier's `affects` selects holds from no source |
| `add_resource` | `(player, name, amount)` | every role | core | runs | adds `amount` of the player resource `name`, one the mode declares, to `player` |
| `add_xp` | `(unit, track, amount)` | every role | progression | runs | gives `unit` `amount` of experience on `track`, one of its unit type's |
| `area` | `(pos)` | action | areas | runs | lands one more of the action's areas at `pos`, its own cast |
| `attack_hit` | `(target)` | action, modifier, AI | combat | runs | an extra attack of the acting unit on `target`: no crit, and no `on_attack` |
| `available` | `(player, choice, value)` | mode | core | runs | whether `player` may choose `value` of `choice`: no other player chose it in a unique choice |
| `avatars` | `() or (team)` | every role | core | runs | the avatars, living or dead, of every team or of `team`, by stable id |
| `chance` | `(p)` | every role | core | planned | true with probability `p`, from the secret stream |
| `charge` | read | action | abilities | planned | how long a charged cast was held, from 0 to 1 |
| `choose` | `(player, choice, values)` | mode | core | runs | records `values`, as many as `choice` takes, each a value it offers, none twice and, in a unique choice, none another player chose, as what `player` chose of it; one value may be given alone |
| `chosen` | `(player, choice)` | mode | core | runs | the values `player` chose of `choice`, in order; empty before the player chose |
| `damage` | `(target, amount, kind)` | every role | combat | runs | deals `amount` of `kind`, one of the mode's `[combat] damage_kinds`, to `target` |
| `dash` | `(unit, to, speed)` | every role | navigation | planned | moves `unit` to `to` at `speed` |
| `end` | `(team) or (())` | mode | core | runs | ends the match, once: `team` wins, `()` is a draw |
| `enemy_team` | `(team)` | every role | core | runs | the one team that is `team`'s enemy, in a mode of two playing teams |
| `find` | `(of, pos, radius, filter)` | every role | core | runs | the living units within `radius` of `pos` that `filter` selects for `of`, seen or not, by stable id |
| `find_visible` | `(of, pos, radius, filter)` | every role | vision | runs | as `find`, of the units `of`'s team sees |
| `grant` | `(unit, kind, ids)` | mode | abilities | runs | puts the actions `ids`, loadout entries the mode depends on, in the slot kind `kind` of `unit`, after its slots of that kind, at the kind's first rank |
| `grant_perk` | `(unit, id)` | every role | progression | planned | gives `unit` the perk `id`, with no point and no requirement |
| `heal` | `(unit, amount)` | every role | combat | runs | heals `unit`'s life pool, times one plus its `heal_scale` stat |
| `knock_back` | `(unit, from, distance, ms)` | every role | stats | planned | pushes `unit` away from `from` |
| `learn` | `(avatar, slot)` | mode | abilities | runs | the ability in `slot` a rank more, up to its last |
| `map` | read | every role | core | runs | the map: its paths and its markers |
| `nearest_visible` | `(of, radius, filter)` | every role | vision | runs | the nearest living unit within `radius` of `of` that `filter` selects and `of`'s team sees, `()` with none |
| `order_attack` | `(unit, target)` | AI | orders | runs | `unit`, which has an attack, attacks `target`, a living enemy |
| `order_follow_path` | `(unit)` | AI | orders | runs | `unit` drops its target and walks its path again |
| `order_move` | `(unit, pos)` | AI | orders | runs | `unit` drops its target and walks to `pos`, within the map, off its path |
| `order_reset` | `(unit)` | AI | orders | runs | `unit` drops its target and walks home, taking no order until there, where its pools fill |
| `origin` | read | action | abilities | planned | where the cast comes from |
| `p` | read | every role | core | runs | the params: an ability's at its rank, a modifier's then its ability's, or the mode's |
| `pick` | `(list)` | every role | core | planned | an entry of `list`, from the secret stream |
| `players` | read | mode | core | runs | how many players the session has |
| `projectile` | `(from, direction) or (from, unit)` | action | projectiles | runs | launches one more of the action's projectiles from `from`, along `direction` or homing on `unit`, its own cast |
| `range` | read | action | abilities | planned | the ability's range at its rank |
| `reduce_cooldown` | `(unit, id, ms)` | every role | abilities | planned | takes `ms` off the cooldown of `unit`'s ability `id` |
| `reduce_cooldowns` | `(unit, fraction)` | every role | abilities | planned | takes `fraction` off the cooldowns of `unit`'s basic abilities |
| `remove` | `(handle)` | every role | stats | runs | ends the modifier, projectile or area at once |
| `respawn` | `(unit, ms)` | mode | combat | runs | brings back `unit`, dead and of a type that stays, `ms` from the call |
| `restore` | `(unit, pool, amount)` | every role | combat | runs | gives `unit` back `amount` of its `pool`, unscaled |
| `reveal` | `(pos, radius, ms)` | every role | vision | planned | shows the source's team what is within `radius` of `pos` |
| `set_relation` | `(a, b, relation)` | every role | core | runs | sets how teams `a` and `b` regard each other, `hostile`, `neutral` or `friendly`, their vision as it was |
| `spawn_group` | `(team, path, from, types)` | mode | core | runs | spawns `types` of `team` in order at the end `from`, `start` or `end`, of `path`, walking it from there |
| `spawn_unit` | `(type, team, pos) or (type, team, pos, player)` | mode | core | runs | spawns a unit of `type` on `team` at `pos`, within the map's bounds, owned by `player` if given, when the call ends; the new unit, for `grant` |
| `state` | read | mode | core | runs | the mode's state fields, by name, to read and write |
| `team_of` | `(player)` | mode | core | runs | the name of `player`'s team |
| `teams` | read | every role | core | runs | the playing teams' names, the teams with slots |
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
| `has_perk` | `(id)` | progression | planned | whether it has the perk `id` |
| `has_tag` | `(tag)` | core | runs | whether it has the tag, of its type or a modifier |
| `is_avatar` | read | core | runs | whether it is an avatar |
| `is_enemy_of` | `(unit)` | core | runs | whether its team may attack the other's, hostile or neutral |
| `level` | read | stats | runs | its level |
| `owner` | read | core | runs | its player's slot, `()` with none |
| `params` | read | core | runs | its unit type's params, unresolved |
| `path` | read | core | runs | the name of the path it walks, `()` with none |
| `points` | read | progression | planned | its unspent points |
| `pool` | `(name)` | stats | runs | the current amount of its pool `name` |
| `pool_max` | `(name)` | stats | runs | the maximum of its pool `name` |
| `pos` | read | core | runs | where it stands |
| `radius` | read | core | runs | its body's radius, 0 with no body |
| `recent_attackers` | `(ms)` | combat | runs | the living units that struck it within the last `ms`, rounded up to whole ticks |
| `spawn_pos` | read | core | runs | where it spawned, where it respawns; `()` with none |
| `stat` | `(name)` | stats | runs | its value of a stat the mode declares |
| `target` | read | core | runs | its attack's target, `()` with none |
| `team` | read | core | runs | its team's name |
| `track_level` | `(track)` | progression | planned | its level on `track` |
| `unit_type` | read | core | runs | its unit type's name |
| `xp` | `(track)` | progression | planned | its experience on `track` |

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

## Hit `hit`

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `delivery` | read | abilities | runs | the projectile or area unit that delivered it, `()` when at once or gone |
| `direction` | read | abilities | runs | the direction its delivery flew in |
| `distance` | read | abilities | runs | how far its delivery flew |
| `part` | read | hitscan | planned | the body part a ray or a sweep struck, `()` with none |
| `pos` | read | abilities | runs | where it hit, or where its delivery ended |
| `target` | read | abilities | runs | the unit the action aimed at, `()` with none |

## Damage `d`

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `ability` | read | combat | runs | the ability that dealt it, `""` when none |
| `amount` | read | combat | runs | raw in `calc_damage`, final in a hook |
| `attack` | read | combat | runs | whether an attack dealt it |
| `extra` | read | combat | runs | whether `ctx.attack_hit` dealt it |
| `kind` | read | combat | runs | one of the mode's `[combat] damage_kinds` |
| `roll` | read | combat | runs | its attack's random number, at least 0 and less than 1, `()` for other damage |
| `source` | read | combat | runs | the unit that dealt it, `()` when gone or none |
| `target` | read | combat | runs | the unit it is dealt to |

## Position

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `direction_to` | `(pos)` | core | runs | the unit vector towards `pos` in the map's metric, `()` for the same point |
| `distance_to` | `(pos)` | core | runs | the distance to `pos` in the map's metric |
| `within` | `(pos, radius)` | core | runs | whether `pos` is within `radius` in the map's metric, exactly: the test for reach |

## Vector

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `*` | operator | core | planned | the vector scaled by a number |
| `+` | operator | core | planned | the sum of two vectors |
| `-` | operator | core | planned | the difference of two vectors |
| `rotated_deg` | `(degrees)` | core | runs | the vector turned by `degrees` about the vertical, counter-clockwise seen from above |

## Map, `ctx.map`

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `markers` | `(tag)` | core | runs | the markers with `tag`, in the map's order |
| `paths` | read | navigation | runs | the paths' names |

## Marker, of `ctx.map.markers(tag)`

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `name` | read | core | runs | its name |
| `params` | read | core | runs | its params, by name |
| `pos` | read | core | runs | its point, `()` for a region |
| `team` | read | core | runs | its team's name, `()` with none |

## Hooks

| Hook | Role | Capability | Status |
| --- | --- | --- | --- |
| `on_resolve(ctx, unit, target)` | action | abilities | runs |
| `on_hit(ctx, unit, target, hit)` | action | abilities | runs |
| `on_end(ctx, unit, hit)` | action | abilities | runs |
| `on_channel_tick(ctx, unit)` | action | abilities | planned |
| `on_interrupt(ctx, unit, target)` | action | abilities | planned |
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
| `calc_heal(ctx, h)` | mode | combat | planned |
| `on_think(ctx, unit)` | AI | orders | runs |
| `on_level_up(ctx, unit, track, level)` | mode | progression | runs |

## Tag effects

| Effect | Status |
| --- | --- |
| `blocks = ["move"]` | runs |
| `blocks = ["use"]` | planned |
| `blocks = ["attack"]` | runs |
| `blocks = ["target"]` | runs |
| `blocks = ["damage"]` | runs |
| `immune = [tags]` | runs |
| `blocks = ["cast"]` | runs |
| `hidden = true` | runs |
| `detects = true` | runs |

## Data fields

### `data/mode.toml`

| Field | Status |
| --- | --- |
| `script` | runs |
| `combat` | runs |
| `navigation` | runs |
| `slots` | runs |
| `choices` | runs |
| `inputs` | runs |
| `state` | runs |
| `params` | runs |
| `modifiers` | runs |
| `actions` | runs |
| `stats` | runs |
| `pools` | runs |
| `resources` | runs |
| `relations` | runs |
| `tags` | runs |
| `state_version` | planned |
| `tracks` | runs |

### The mode's `[combat]`

| Field | Status |
| --- | --- |
| `damage_kinds` | runs |
| `assist_window_ms` | runs |
| `life` | runs |
| `leech` | runs |
| `heal_scale` | runs |

### The mode's `[navigation]`

| Field | Status |
| --- | --- |
| `layers` | runs |

### A slot kind, `[[slots]]`

| Field | Status |
| --- | --- |
| `name` | runs |
| `ranks` | runs |
| `levels` | planned |

### A choice, `[choices.<name>]`

| Field | Status |
| --- | --- |
| `offers` | runs |
| `unique` | runs |
| `count` | runs |
| `slot` | runs |

### The mode's `[combat] leech`

| Field | Status |
| --- | --- |
| `attack` | runs |
| `other` | runs |

### A pair of teams, `[[relations]]`

| Field | Status |
| --- | --- |
| `teams` | runs |
| `relation` | runs |
| `vision` | runs |

### An action, `[actions.<id>]`

| Field | Status |
| --- | --- |
| `kind` | runs |
| `targeting` | runs |
| `range` | runs |
| `cost` | runs |
| `windup_ms` | runs |
| `passive_modifier` | runs |
| `passive_while_ready` | runs |
| `rate` | runs |
| `damage` | runs |
| `damage_kind` | runs |
| `script` | runs |
| `cooldown_ms` | runs |
| `params` | runs |
| `clamp_to_range` | planned |
| `toggle` | planned |
| `channel` | planned |
| `hold` | planned |
| `charges` | planned |
| `charge` | planned |
| `projectile_state` | planned |
| `unit_type` | runs |
| `delivery` | runs |

### An action's `delivery`

| Field | Status |
| --- | --- |
| `projectile` | runs |
| `count` | runs |
| `spread_deg` | runs |
| `area` | runs |

### A unit type's `projectile`

| Field | Status |
| --- | --- |
| `speed` | runs |
| `width` | runs |
| `range` | runs |
| `homing` | runs |
| `stop_on_hit` | runs |
| `once_per_cast` | runs |
| `hits` | runs |
| `gravity` | planned |
| `sight_radius` | planned |
| `collide` | planned |

### A unit type's `area`

| Field | Status |
| --- | --- |
| `radius` | runs |
| `delay_ms` | runs |
| `duration_ms` | runs |
| `affects` | runs |
| `inside` | runs |

### An area's `inside`

| Field | Status |
| --- | --- |
| `self` | runs |
| `allies` | runs |
| `enemies` | runs |

### A track, `[tracks.<name>]`

| Field | Status |
| --- | --- |
| `levels` | runs |
| `level` | runs |

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
| `affects` | runs |
| `params` | runs |
| `state` | runs |
| `tags` | runs |

### A modifier's `aura`

| Field | Status |
| --- | --- |
| `radius` | runs |
| `affects` | runs |
| `modifier` | runs |

### A unit type's `combat`

| Field | Status |
| --- | --- |
| `on_death` | runs |

### A unit type's `production`

| Field | Status |
| --- | --- |
| `queue` | runs |

### A unit type's `vision`

| Field | Status |
| --- | --- |
| `sight_range` | runs |

### A unit type's `collision`

| Field | Status |
| --- | --- |
| `radius` | runs |
| `layer` | runs |

### A unit type's `orders`

| Field | Status |
| --- | --- |
| `ai` | runs |
| `think_ms` | runs |

### A tag's effects, `[tags.<name>]`

| Field | Status |
| --- | --- |
| `blocks` | runs |
| `hidden` | runs |
| `detects` | runs |
| `immune` | runs |
