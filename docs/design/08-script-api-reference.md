# Campfire — Script API reference

Generated from the script API's registry ([One source](08-script-api.md#one-source)); do not edit it. A test fails when it differs from what the registry writes; run that test with `CAMPFIRE_BLESS=1` to write it again. A name that runs is bound by the code that runs it; a planned one is one design 08 gives that the release does not run yet. The rules of the API are [design 08](08-script-api.md).

## `ctx`

| Name | Form | Roles | Capability | Status | What it is |
| --- | --- | --- | --- | --- | --- |
| `add_charge` | `(unit, id)` | every role | abilities | planned | gives `unit`'s ability `id` a charge |
| `add_modifier` | `(unit, id) or (unit, id, duration_ms)`, `id` a modifier | every role | stats | since 1.0 | applies the modifier `id` of the script's package to `unit` from the acting unit, with the call's action at its rank, which gives each param the modifier reads and does not declare, and returns its handle |
| `add_player_modifier` | `(player, id)`, `id` a modifier | every role | stats | since 1.0 | gives `player` the modifier `id` of the script's package, which every living unit it owns that the modifier's `affects` selects holds from no source and with no action, so the modifier declares each param it reads |
| `add_resource` | `(player, name, amount)`, `name` a player resource | every role | core | since 1.0 | adds `amount` of the player resource `name`, one the mode declares, to `player` |
| `add_xp` | `(unit, track, amount)`, `track` a track | every role | progression | since 1.0 | gives `unit` `amount` of experience on `track`, one of its unit type's |
| `area` | `(pos)` | action | areas | since 1.0 | lands one more of the action's areas at `pos`, its own cast; the new area, which spawns later in the tick |
| `attack_hit` | `(target)` | action, modifier, AI | combat | since 1.0 | an extra attack of the acting unit on `target`: no crit, and no `on_attack` |
| `available` | `(player, choice, value)`, `choice` a choice | mode | core | since 1.0 | whether `player` may choose `value` of `choice`: no other player chose it in a unique choice |
| `avatars` | `() or (team)` | every role | core | since 1.0 | the avatars, living or dead, of every team or of `team`, by stable id |
| `carry` | read | mode | core | planned | the carry the session loaded, which the mode writes for the next session |
| `chance` | `(p)` | every role | core | planned | true with probability `p`, from the secret stream |
| `charge` | read | action | abilities | planned | how long a charged cast was held, from 0 to 1 |
| `choose` | `(player, choice, values)`, `choice` a choice | mode | core | since 1.0 | records `values`, as many as `choice` takes, each a value it offers, none twice and, in a unique choice, none another player chose, as what `player` chose of it; one value may be given alone |
| `chosen` | `(player, choice)`, `choice` a choice | mode | core | since 1.0 | the values `player` chose of `choice`, in order; empty before the player chose |
| `damage` | `(target, amount, kind)`, `kind` a damage kind | every role | combat | since 1.0 | deals `amount` of `kind`, one of the mode's `[combat] damage_kinds`, to `target` |
| `dash` | `(unit, to, speed)` | every role | navigation | planned | moves `unit` to `to` at `speed` |
| `end` | `(team) or (())` | mode | core | since 1.0 | ends the match, once: `team` wins, `()` is a draw |
| `enemy_team` | `(team)`, `team` a team | every role | core | since 1.0 | the one team that is `team`'s enemy, in a mode of two playing teams |
| `find` | `(of, pos, radius, filter)`, `filter` a filter | every role | core | since 1.0 | the living targets whose bodies come within `radius` of `pos`, as an area's, that `filter` selects for `of`, seen or not, by stable id |
| `find_visible` | `(of, pos, radius, filter)`, `filter` a filter | every role | vision | since 1.0 | as `find`, of the units `of`'s team sees |
| `generate` | `(region)` | mode | core | planned | builds the map's region `region` through `on_generate` |
| `grant` | `(unit, kind, ids)`, `kind` a slot kind | mode | abilities | since 1.0 | puts the actions `ids`, loadout entries the mode depends on, in the slot kind `kind` of `unit`, after its slots of that kind, at the kind's first rank |
| `grant_perk` | `(unit, id)` | every role | progression | planned | gives `unit` the perk `id`, with no point and no requirement |
| `heal` | `(unit, amount)` | every role | combat | since 1.0 | heals `unit`'s life pool, times one plus its `heal_scale` stat |
| `knock_back` | `(unit, from, distance, ms)` | every role | stats | planned | pushes `unit` away from `from` |
| `learn` | `(avatar, slot)` | mode | abilities | since 1.0 | the ability in `slot` a rank more, up to its last |
| `map` | read | every role | core | since 1.0 | the map: its paths and its markers |
| `nearest_visible` | `(of, radius, filter)`, `filter` a filter | every role | vision | since 1.0 | the nearest living target, centre to centre, whose body `radius` from the edge of `of`'s reaches, as a weapon's range, that `filter` selects and `of`'s team sees, `()` with none |
| `order_attack` | `(unit, target)` | AI | orders | since 1.0 | `unit` attacks `target`, a living enemy that one of its weapons selects |
| `order_follow_path` | `(unit)` | AI | orders | since 1.0 | `unit` drops its target and walks its path again |
| `order_move` | `(unit, pos)` | AI | orders | since 1.0 | `unit` drops its target and walks to `pos`, within the map, off its path |
| `order_reset` | `(unit)` | AI | orders | since 1.0 | `unit` drops its target and walks home, taking no order until there, where its pools fill |
| `origin` | read | action | abilities | planned | where the cast comes from |
| `p` | read | every role | core | since 1.0 | the params: an ability's at its rank, a modifier's then its ability's, or the mode's |
| `pick` | `(list)` | every role | core | planned | an entry of `list`, from the secret stream |
| `players` | read | mode | core | since 1.0 | how many players the session has |
| `projectile` | `(from, direction) or (from, unit)` | action | projectiles | since 1.0 | launches one more of the action's projectiles from `from`, its own cast: along `direction` for a line type, or homing on `unit` for a homing type; the new projectile, which spawns later in the tick |
| `range` | read | action | abilities | planned | the ability's range at its rank |
| `reduce_cooldown` | `(unit, id, ms)` | every role | abilities | planned | takes `ms` off the cooldown of `unit`'s ability `id` |
| `reduce_cooldowns` | `(unit, fraction)` | every role | abilities | planned | takes `fraction` off the cooldowns of `unit`'s basic abilities |
| `remove` | `(handle)` | every role | stats | since 1.0 | ends the modifier, projectile or area at once |
| `respawn` | `(unit, ms)` | mode | combat | since 1.0 | brings back `unit`, dead and of a type that stays, `ms` from the call |
| `restore` | `(unit, pool, amount)`, `pool` a pool | every role | combat | since 1.0 | gives `unit` back `amount` of its `pool`, unscaled |
| `reveal` | `(pos, radius, ms)` | every role | vision | planned | shows the source's team what is within `radius` of `pos` |
| `save` | `()` | mode | core | planned | asks for a save at the end of the tick |
| `set_relation` | `(a, b, relation)`, `a` a team, `b` a team, `relation` a `Relation` | every role | core | since 1.0 | sets how teams `a` and `b` regard each other, their vision as it was |
| `spawn_group` | `(team, path, from, types)`, `team` a team, `path` a path, `from` a `PathEnd` | mode | core | since 1.0 | spawns `types` of `team` in order at the end `from` of `path`, walking it from there |
| `spawn_unit` | `(type, team, pos) or (type, team, pos, player)`, `type` a unit type, `team` a team | mode | core | since 1.0 | spawns a unit of `type` on `team` at `pos`, within the map's bounds, owned by `player` if given, when the call ends; the new unit, for `grant` and its `.state` |
| `state` | read | mode | core | since 1.0 | the mode's state fields, by name, to read and write |
| `team_of` | `(player)` | mode | core | since 1.0 | the name of `player`'s team |
| `teams` | read | every role | core | since 1.0 | the playing teams' names, the teams with slots |
| `teleport` | `(unit, pos)` | every role | navigation | planned | puts `unit` at `pos` |
| `timer` | `(name, ms, repeat, data)` | mode | core | since 1.0 | calls `on_timer` `ms` from the call, rounded up to whole ticks, at least one |
| `units_tagged` | `(tag)`, `tag` a tag | every role | core | since 1.0 | the units of a tag, living or dead, by stable id |

## Unit

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `!=` | operator | core | since 1.0 | whether the two are two units |
| `==` | operator | core | since 1.0 | whether the two are one unit |
| `alive` | read | core | since 1.0 | whether it lives |
| `attack_range` | read | combat | since 1.0 | its attack's range |
| `can_see` | `(unit)` | vision | since 1.0 | whether its team sees the other unit |
| `has_modifier` | `(id)`, `id` a modifier | stats | since 1.0 | whether it carries the modifier of the script's package |
| `has_perk` | `(id)` | progression | planned | whether it has the perk `id` |
| `has_tag` | `(tag)`, `tag` a tag | core | since 1.0 | whether it has the tag, of its type or a modifier |
| `is_avatar` | read | core | since 1.0 | whether it is an avatar |
| `is_enemy_of` | `(unit)` | core | since 1.0 | whether its team may attack the other's, hostile or neutral |
| `level` | read | stats | since 1.0 | its level |
| `owner` | read | core | since 1.0 | its player's slot, `()` with none |
| `params` | read | core | since 1.0 | its unit type's params, unresolved |
| `path` | read | navigation | since 1.0 | the name of the path it walks, `()` with none |
| `points` | read | progression | planned | its unspent points |
| `pool` | `(name)`, `name` a pool | stats | since 1.0 | the current amount of its pool `name` |
| `pool_max` | `(name)`, `name` a pool | stats | since 1.0 | the maximum of its pool `name` |
| `pos` | read | core | since 1.0 | where it stands |
| `radius` | read | core | since 1.0 | its body's radius, 0 with no body |
| `recent_attackers` | `(ms)` | combat | since 1.0 | the living units that struck it within the last `ms`, rounded up to whole ticks |
| `spawn_pos` | read | core | since 1.0 | where it spawned, where it respawns; `()` with none |
| `stat` | `(name)`, `name` a stat | stats | since 1.0 | its value of a stat the mode declares |
| `state` | read | core | since 1.0 | its script state, by name, which a call may write and read back |
| `target` | read | core | since 1.0 | its attack's target, `()` with none |
| `team` | read | core | since 1.0 | its team's name |
| `track_level` | `(track)` | progression | planned | its level on `track` |
| `unit_type` | read | core | since 1.0 | its unit type's name |
| `xp` | `(track)` | progression | planned | its experience on `track` |

## New unit, of `spawn_unit`, `ctx.projectile` and `ctx.area`

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `state` | read | core | since 1.0 | its script state, by name, at its type's defaults but what the call wrote, which applies as it spawns |

## Modifier `m`

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `carrier` | read | stats | since 1.0 | the unit that carries it |
| `source` | read | stats | since 1.0 | the unit that applied it, `()` when gone or none |
| `stacks` | written and read | stats | since 1.0 | its stacks, which a call may write and read back |
| `state` | read | stats | since 1.0 | its script state, by name, which a call may write and read back |

## Hit `hit`

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `delivery` | read | abilities | since 1.0 | the projectile or area unit that delivered it, `()` when at once or gone |
| `direction` | read | abilities | since 1.0 | the direction its delivery flew in, `()` for an area or a delivery that did not move |
| `distance` | read | abilities | since 1.0 | how far its delivery flew |
| `part` | read | hitboxes | planned | the body part a ray or a sweep struck, `()` with none |
| `pos` | read | abilities | since 1.0 | where it hit, or where its delivery ended |
| `target` | read | abilities | since 1.0 | the unit the action aimed at, `()` with none |

## Damage `d`

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `ability` | read | combat | since 1.0 | the action that dealt it: an ability, or an attack's weapon; `()` for none |
| `amount` | read | combat | since 1.0 | raw in `calc_damage`, final in a hook |
| `attack` | read | combat | since 1.0 | whether an attack dealt it |
| `extra` | read | combat | since 1.0 | whether `ctx.attack_hit` dealt it |
| `hit` | read | combat | since 1.0 | how its projectile or area reached the target, `()` for damage none delivered |
| `kind` | read | combat | since 1.0 | one of the mode's `[combat] damage_kinds` |
| `roll` | read | combat | since 1.0 | its attack's random number, at least 0 and less than 1, `()` for other damage |
| `source` | read | combat | since 1.0 | the unit that dealt it, `()` when gone or none |
| `target` | read | combat | since 1.0 | the unit it is dealt to |

## Heal `h`

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `ability` | read | combat | since 1.0 | the ability that gave it, `()` for none |
| `amount` | read | combat | since 1.0 | before `calc_heal` and the heal scale |
| `leech` | read | combat | since 1.0 | whether its source's leech gave it |
| `source` | read | combat | since 1.0 | the unit that gave it, `()` when gone or none |
| `target` | read | combat | since 1.0 | the unit it heals |

## Position

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `direction_to` | `(pos)` | core | since 1.0 | the unit vector towards `pos` in the map's metric, `()` for the same point |
| `distance_to` | `(pos)` | core | since 1.0 | the distance to `pos` in the map's metric |
| `within` | `(pos, radius)` | core | since 1.0 | whether `pos` is within `radius` in the map's metric, exactly: the reach rule between two points, which have no bodies |

## Vector

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `*` | operator | core | planned | the vector scaled by a number |
| `+` | operator | core | planned | the sum of two vectors |
| `-` | operator | core | planned | the difference of two vectors |
| `rotated_deg` | `(degrees)` | core | since 1.0 | the vector turned by `degrees` about the vertical, counter-clockwise seen from above |

## Map, `ctx.map`

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `markers` | `(tag)`, `tag` a marker with tag | core | since 1.0 | the markers with `tag`, in the map's order |
| `paths` | read | navigation | since 1.0 | the paths' names |

## Marker, of `ctx.map.markers(tag)`

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `name` | read | core | since 1.0 | its name |
| `params` | read | core | since 1.0 | its params, by name |
| `pos` | read | core | since 1.0 | its point, `()` for a region |
| `team` | read | core | since 1.0 | its team's name, `()` with none |

## Engine enums

Each enum's module holds its members, and the function `named`, which gives the member a text names as data does; a member has `==`, `!=` and `to_string`, its name in data.

| Enum | Members |
| --- | --- |
| `Relation` | `Hostile`, `Neutral`, `Friendly` |
| `PathEnd` | `Start`, `End` |

## Hooks

| Hook | Role | Capability | Status |
| --- | --- | --- | --- |
| `on_resolve(ctx, unit, target)` | action | abilities | since 1.0 |
| `on_hit(ctx, unit, target, hit)` | action | abilities | since 1.0 |
| `on_end(ctx, unit, hit)` | action | abilities | since 1.0 |
| `on_channel_tick(ctx, unit)` | action | abilities | planned |
| `on_interrupt(ctx, unit, target)` | action | abilities | planned |
| `on_interval(ctx, m)` | modifier | stats | since 1.0 |
| `on_attack(ctx, m, target)` | modifier | combat | since 1.0 |
| `on_attack_hit(ctx, m, d)` | modifier | combat | since 1.0 |
| `on_damage_taken(ctx, m, d)` | modifier | combat | since 1.0 |
| `on_kill(ctx, m, victim)` | modifier | combat | since 1.0 |
| `on_takedown(ctx, m, victim)` | modifier | combat | since 1.0 |
| `on_match_start(ctx)` | mode | core | since 1.0 |
| `on_mode_input(ctx, player, name, value)` | mode | core | since 1.0 |
| `on_timer(ctx, name, data)` | mode | core | since 1.0 |
| `on_player_join(ctx, player)` | mode | core | planned |
| `on_player_leave(ctx, player)` | mode | core | planned |
| `on_unit_died(ctx, unit, killer, assisters)` | mode | combat | since 1.0 |
| `calc_damage(ctx, d)` | mode | combat | since 1.0 |
| `calc_heal(ctx, h)` | mode | combat | since 1.0 |
| `on_think(ctx, unit)` | AI | orders | since 1.0 |
| `on_level_up(ctx, unit, track, level)` | mode | progression | since 1.0 |
| `on_generate(ctx, region)` | mode | core | planned |

## Tag effects

| Effect | Status |
| --- | --- |
| `blocks = ["move"]` | since 1.0 |
| `blocks = ["use"]` | planned |
| `immune = [tags]` | since 1.0 |
| `blocks = ["attack"]` | since 1.0 |
| `blocks = ["target"]` | since 1.0 |
| `blocks = ["damage"]` | since 1.0 |
| `hidden = true` | since 1.0 |
| `detects = true` | since 1.0 |
| `blocks = ["cast"]` | since 1.0 |

## Data fields

### `data/mode.toml`

| Field | Status |
| --- | --- |
| `tracks` | since 1.0 |
| `script` | since 1.0 |
| `combat` | since 1.0 |
| `navigation` | since 1.0 |
| `slots` | since 1.0 |
| `choices` | since 1.0 |
| `inputs` | since 1.0 |
| `state` | since 1.0 |
| `params` | since 1.0 |
| `modifiers` | since 1.0 |
| `actions` | since 1.0 |
| `stats` | since 1.0 |
| `pools` | since 1.0 |
| `resources` | since 1.0 |
| `relations` | since 1.0 |
| `tags` | since 1.0 |
| `state_version` | planned |

### The mode's `[combat]`

| Field | Status |
| --- | --- |
| `damage_kinds` | since 1.0 |
| `assist_window_ms` | since 1.0 |
| `life` | since 1.0 |
| `leech` | since 1.0 |
| `heal_scale` | since 1.0 |

### The mode's `[navigation]`

| Field | Status |
| --- | --- |
| `layers` | since 1.0 |

### A slot kind, `[[slots]]`

| Field | Status |
| --- | --- |
| `name` | since 1.0 |
| `ranks` | since 1.0 |
| `levels` | planned |

### A choice, `[choices.<name>]`

| Field | Status |
| --- | --- |
| `offers` | since 1.0 |
| `unique` | since 1.0 |
| `count` | since 1.0 |
| `slot` | since 1.0 |

### The mode's `[combat] leech`

| Field | Status |
| --- | --- |
| `attack` | since 1.0 |
| `other` | since 1.0 |

### A pair of teams, `[[relations]]`

| Field | Status |
| --- | --- |
| `teams` | since 1.0 |
| `relation` | since 1.0 |
| `vision` | since 1.0 |

### An action, `[actions.<id>]`

| Field | Status |
| --- | --- |
| `kind` | since 1.0 |
| `targeting` | since 1.0 |
| `range` | since 1.0 |
| `cost` | since 1.0 |
| `windup_ms` | since 1.0 |
| `passive_modifier` | since 1.0 |
| `passive_while_ready` | since 1.0 |
| `params` | since 1.0 |
| `on_resolve` | since 1.0 |
| `on_hit` | since 1.0 |
| `on_end` | since 1.0 |
| `rate` | since 1.0 |
| `damage` | since 1.0 |
| `damage_kind` | since 1.0 |
| `delivery` | since 1.0 |
| `script` | since 1.0 |
| `cooldown_ms` | since 1.0 |
| `clamp_to_range` | planned |
| `toggle` | planned |
| `channel` | planned |
| `hold` | planned |
| `charges` | planned |
| `charge` | planned |
| `unit_type` | since 1.0 |

### An effect of an action's `on_resolve`, `on_hit` or `on_end`

| Field | Status |
| --- | --- |
| `damage` | since 1.0 |
| `heal` | since 1.0 |
| `restore` | since 1.0 |
| `modifier` | since 1.0 |
| `xp` | since 1.0 |
| `purge` | since 1.0 |
| `launch` | since 1.0 |
| `to` | since 1.0 |
| `spawn` | planned |
| `move` | planned |
| `loot` | planned |
| `noise` | planned |

### An action's `delivery`

| Field | Status |
| --- | --- |
| `projectile` | since 1.0 |
| `count` | since 1.0 |
| `spread_deg` | since 1.0 |
| `area` | since 1.0 |

### A unit type's `projectile`

| Field | Status |
| --- | --- |
| `speed` | since 1.0 |
| `width` | since 1.0 |
| `range` | since 1.0 |
| `homing` | since 1.0 |
| `stop_on_hit` | since 1.0 |
| `once_per_cast` | since 1.0 |
| `hits` | since 1.0 |
| `gravity` | planned |

### A unit type's `area`

| Field | Status |
| --- | --- |
| `radius` | since 1.0 |
| `delay_ms` | since 1.0 |
| `duration_ms` | since 1.0 |
| `affects` | since 1.0 |
| `inside` | since 1.0 |

### An area's `inside`

| Field | Status |
| --- | --- |
| `self` | since 1.0 |
| `allies` | since 1.0 |
| `enemies` | since 1.0 |

### A track, `[tracks.<name>]`

| Field | Status |
| --- | --- |
| `levels` | since 1.0 |
| `level` | since 1.0 |

### A modifier, `[modifiers.<id>]`

| Field | Status |
| --- | --- |
| `script` | since 1.0 |
| `duration_ms` | since 1.0 |
| `interval_ms` | since 1.0 |
| `stacks_expire_ms` | since 1.0 |
| `reapply` | since 1.0 |
| `max_stacks` | since 1.0 |
| `stats` | since 1.0 |
| `shield` | since 1.0 |
| `aura` | since 1.0 |
| `affects` | since 1.0 |
| `params` | since 1.0 |
| `state` | since 1.0 |
| `tags` | since 1.0 |

### A modifier's `aura`

| Field | Status |
| --- | --- |
| `radius` | since 1.0 |
| `affects` | since 1.0 |
| `modifier` | since 1.0 |

### A unit type's `combat`

| Field | Status |
| --- | --- |
| `on_death` | since 1.0 |

### A unit type's `production`

| Field | Status |
| --- | --- |
| `queue` | since 1.0 |

### A unit type's `vision`

| Field | Status |
| --- | --- |
| `sight_range` | since 1.0 |

### A unit type's `collision`

| Field | Status |
| --- | --- |
| `radius` | since 1.0 |
| `layer` | since 1.0 |

### A unit type's `orders`

| Field | Status |
| --- | --- |
| `ai` | since 1.0 |
| `think_ms` | since 1.0 |

### A tag's effects, `[tags.<name>]`

| Field | Status |
| --- | --- |
| `blocks` | since 1.0 |
| `hidden` | since 1.0 |
| `detects` | since 1.0 |
| `immune` | since 1.0 |
