# Campfire — Script API reference

Generated from the script API's registry ([One source](06-script-api.md#one-source)); do not edit it. A test fails when it differs from what the registry writes; run that test with `CAMPFIRE_BLESS=1` to write it again. A name that runs is bound by the code that runs it; a planned one is one the design gives that the release does not run yet. The rules of the API are in [Script API](06-script-api.md).

## `ctx`

| Name | Form | Roles | Capability | Status | What it is |
| --- | --- | --- | --- | --- | --- |
| `add_charge` | `(unit: Unit, id: text)`, `id` an action | every role | abilities | since 1.0 | gives `unit`'s ability `id`, of the script's package, a charge, up to its most |
| `add_modifier` | `(unit: Unit, id: text)` or `(unit: Unit, id: text, duration_ms: ms)`, `id` a modifier | every role | stats | since 1.0 | applies the modifier `id` of the script's package to `unit` from the acting unit, with the call's action at its rank, which gives each param the modifier reads and does not declare, and returns its handle |
| `add_player_modifier` | `(player: Player, id: text)`, `id` a modifier | every role | stats | since 1.0 | gives `player` the modifier `id` of the script's package, which every living unit it owns that the modifier's `affects` selects holds from no source and with no action, so the modifier declares each param it reads |
| `add_resource` | `(player: Player, name: text, amount: int)`, `name` a player resource | every role | core | since 1.0 | adds `amount`, at least 0, of the player resource `name`, one the mode declares, to `player` |
| `add_xp` | `(unit: Unit, track: text, amount: Num)`, `track` a track | every role | progression | since 1.0 | gives `unit` `amount` of experience on `track`, one of its unit type's |
| `after` | `(name: text, ms: ms)` or `(name: text, ms: ms, data: any)`, `name` a timer | mode | core | since 1.0 | calls `on_timer` with `name`, one the mode declares, and `data`, `()` if not given, `ms` from the call, rounded up to whole ticks, at least one |
| `all_avatars` | `()` or `(team: text)`, `team` a team | every role | core | since 1.0 | the avatars, living or dead, of every team or of `team`, by stable id |
| `all_units_tagged` | `(tag: text)`, `tag` a tag | every role | core | since 1.0 | the units of a tag, living or dead, by stable id |
| `attack_hit` | `(target: Unit)` | action, modifier, AI | combat | since 1.0 | an extra attack of the acting unit on `target`, with its first weapon whose filter selects `target`, struck at once: no crit, no `on_attack`, and nothing when no weapon selects it |
| `avatars` | `()` or `(team: text)`, `team` a team | every role | core | since 1.0 | the living avatars of every team or of `team`, by stable id |
| `can_choose` | `(player: Player, choice: text, value: text)`, `choice` a choice | mode | core | since 1.0 | whether `player` may choose `value` of `choice`: no other player chose it in a unique choice |
| `cancel_timer` | `(name: text)`, `name` a timer | mode | core | since 1.0 | ends every timer of `name` the call's script set, those the call set before among them |
| `carry` | read | mode | core | planned | the carry the session loaded, which the mode writes for the next session |
| `chance` | `(p: Num)` | every role | core | since 1.0 | true with probability `p`, from 0 to 1, from the secret stream |
| `charged` | read | action | abilities | since 1.0 | the share of its most a charged action charged, from 0 to 1 |
| `choose` | `(player: Player, choice: text, values: array or text)`, `choice` a choice | mode's own script | core | since 1.0 | records `values`, as many as `choice` takes, each a value it offers, none twice and, in a unique choice, none another player chose, as what `player` chose of it; one value may be given alone |
| `chosen` | `(player: Player, choice: text)`, `choice` a choice | mode | core | since 1.0 | the values `player` chose of `choice`, in order; empty before the player chose |
| `cut_cooldown` | `(unit: Unit, action: text, ms: ms)`, `action` an action | every role | abilities | since 1.0 | takes `ms` off the cooldown of `unit`'s action `action`, of the script's package |
| `damage` | `(unit: Unit, kind: text, amount: Num)` or `(unit: Unit, kind: text, amount: Num, source: Unit)`, `kind` a damage kind | every role | combat | since 1.0 | deals `amount` of `kind`, one of the mode's `[combat] damage_kinds`, to `unit`, from `source`, else from the acting unit |
| `dash` | `(unit: Unit, to: Vector or Unit, speed: Num)` | every role | navigation | since 1.0 | moves `unit` on the ground plane at `speed` meters a second to `to`: a point, or a unit it follows until their bodies touch |
| `draw` | `()` | mode's own script | core | since 1.0 | ends the match with no winner; a call once it ended does nothing |
| `drop` | `(unit, slot)` | every role | items | planned | drops the item in `unit`'s inventory slot `slot` at its feet |
| `enemies` | `(team: text)`, `team` a team, gives a list of team names | every role | core | since 1.0 | the playing teams' names that `team` may attack, hostile or neutral, in order |
| `every` | `(name: text, ms: ms)` or `(name: text, ms: ms, data: any)`, `name` a timer | mode | core | since 1.0 | calls `on_timer` as `after` does, and again each `ms` after, until its cancel |
| `find` | `(of: Unit, position: Vector, radius: Num, filter: text)`, `filter` a filter | every role | core | since 1.0 | the living targets whose bodies come within `radius` of `position`, as an area's, that `filter` selects for `of`, seen or not, by stable id |
| `find_visible` | `(of: Unit, position: Vector, radius: Num, filter: text)`, `filter` a filter | every role | vision | since 1.0 | as `find`, of the units `of`'s team sees |
| `generate` | `(region)` | mode | core | planned | builds the map's region `region` through `on_generate` |
| `give_item` | `(unit, id)` | every role | items | planned | gives `unit` the item `id`, in its first slot that takes it |
| `grant` | `(unit: Unit, kind: text, ids: array)`, `kind` a slot kind | mode | abilities | since 1.0 | puts the actions `ids`, loadout entries the mode depends on, in the slot kind `kind` of `unit`, after its slots of that kind, at the kind's first rank |
| `grant_perk` | `(unit, id)` | every role | progression | planned | gives `unit` the perk `id`, with no point and no requirement |
| `heal` | `(unit: Unit, amount: Num)` | every role | combat | since 1.0 | heals `unit`'s life pool, times one plus its `heal_scale` stat |
| `kill` | `(unit: Unit)` | mode | combat | since 1.0 | takes `unit`'s life whole: it dies as the next tick's deaths find it, with no killer; nothing for a unit that does not live |
| `knock_back` | `(unit: Unit, from: Vector, distance: Num, ms: ms)` | every role | navigation | since 1.0 | moves `unit` `distance` straight away from `from` over `ms` |
| `land` | `(position: Vector)` | action | areas | since 1.0 | lands one more of the action's areas at `position`, its own cast; the new area, which spawns later in the tick |
| `launch` | `(from: Vector, direction: Vector or Unit)` or `(from: Vector, unit: Vector or Unit)` | action | projectiles | since 1.0 | launches one more of the action's projectiles from `from`, its own cast: along `direction` for a line type, homing on `unit` for a homing type, or on a curve to `unit`'s center for a curve type; the new projectile, which spawns later in the tick |
| `learn` | `(avatar: Unit, slot: int)` | mode | abilities | since 1.0 | the ability in `slot` a rank more, up to its last |
| `map` | read | every role | core | since 1.0 | the map: its paths and its markers |
| `nearest_visible` | `(of: Unit, radius: Num, filter: text)`, `filter` a filter | every role | vision | since 1.0 | the nearest living target, center to center, whose body `radius` from the edge of `of`'s reaches, as a weapon's range, that `filter` selects and `of`'s team sees, `()` with none |
| `offers` | `(choice: text)`, `choice` a choice | mode | core | since 1.0 | the values `choice` offers, in order: the avatars in the order of the mode's dependencies, the loadout entries by id, or the values it lists |
| `order_action` | `(slot, target)` | AI | orders | planned | the unit that thinks casts the action in its slot `slot` at `target`, as a player's action order does |
| `order_attack` | `(target: Unit)` | AI | orders | since 1.0 | the unit that thinks attacks `target`, a living enemy that one of its weapons selects |
| `order_follow_path` | `()` | AI | orders | since 1.0 | the unit that thinks drops its target and walks its path again |
| `order_move` | `(position: Vector)` | AI | orders | since 1.0 | the unit that thinks drops its target and walks to `position`, within the map, off its path |
| `order_reset` | `()` | AI | orders | since 1.0 | the unit that thinks drops its target and walks home, taking no order until there, where its pools fill |
| `origin` | read | action | abilities | since 1.0 | where the action's unit stood as it started |
| `params` | read | every role | core | since 1.0 | the params: an ability's at its rank, a modifier's then its ability's, or the mode's |
| `pick` | `(list: array)` | every role | core | since 1.0 | an entry of `list`, each as likely, from the secret stream |
| `players` | read | every role | core | since 1.0 | the session's players, by slot, whatever controls each |
| `purge` | `(unit, tag)` | every role | stats | planned | ends every modifier on `unit` that grants `tag` |
| `quest` | `(player, id)` | every role | quests | planned | the stage of `player`'s quest `id`, `()` before it starts |
| `quest_stage` | `(player, id, stage)` | every role | quests | planned | sets the stage of `player`'s quest `id` |
| `range` | read | action | abilities | since 1.0 | the ability's range at its rank in meters, `()` for a global one |
| `relation` | `(a: text, b: text)`, `a` a team, `b` a team | every role | core | since 1.0 | how team `a` regards team `b`, as the match began the call |
| `remove_modifier` | `(m: Modifier)` | every role | stats | since 1.0 | ends the modifier of `m` at once |
| `remove_player_modifier` | `(player: Player, id: text)`, `id` a modifier | every role | stats | since 1.0 | takes the modifier `id` of the script's package from `player`, and so from each unit it owns, if it holds it |
| `respawn` | `(unit: Unit, ms: ms)` | mode | combat | since 1.0 | brings back `unit`, dead and of a type that stays, `ms` from the call |
| `restore` | `(unit: Unit, pool: text, amount: Num)`, `pool` a pool | every role | stats | since 1.0 | gives `unit` back `amount` of its `pool`, unscaled, when it lives |
| `result` | read | mode | core | since 1.0 | how the match ended, before the call or by its `ctx.win` or `ctx.draw`; `()` while it runs |
| `reveal` | `(position: Vector, radius: Num, ms: ms)` or `(position: Vector, radius: Num, ms: ms, team: text)`, `team` a team | every role | vision | since 1.0 | shows `team`'s vision group, else the acting unit's, the cells within `radius` of `position` for `ms`, from this tick's Vision stage; no hidden unit |
| `save` | `()` | mode | core | since 1.0 | asks for a save at the end of the tick |
| `scale_cooldowns` | `(unit: Unit, kind: text, fraction: Num)`, `kind` a slot kind | every role | abilities | since 1.0 | takes `fraction` of what is left off the cooldowns of `unit`'s abilities in the slot kind `kind` |
| `set_bounty_share` | `(player: Player, share: Num)` | mode | core | since 1.0 | sets `player`'s share, 0 or more, of the cost of each unit its units kill that a death pays, as a power raises it, of a mode of a `[bounty]` |
| `set_out` | `(player: Player, out: bool)` | mode | core | since 1.0 | holds `player` out of play while `out`, as when every avatar it holds is dead |
| `set_relation` | `(a: text, b: text, relation: Relation)`, `a` a team, `b` a team, `relation` a `Relation` | every role | core | since 1.0 | sets how teams `a` and `b` regard each other, their vision as it was |
| `spawn_group` | `(team: text, path: text, from: PathEnd, types: array)`, `team` a team, `path` a path, `from` a `PathEnd` | mode | core | since 1.0 | spawns `types` of `team` in order at the end `from` of `path`, walking it from there |
| `spawn_unit` | `(type: text, team: text, position: Vector)` or `(type: text, team: text, position: Vector, player: Player)`, `type` a unit type, `team` a team | mode | core | since 1.0 | spawns a unit of `type` on `team` at `position`, within the map's bounds, owned by `player` if given, who plays on `team`, when the call ends; the new unit, for `grant` and its `.state` |
| `state` | read | mode's own script | core | since 1.0 | the mode's state fields, by name, to read and write |
| `take_resource` | `(player: Player, name: text, amount: int)`, `name` a player resource | every role | core | since 1.0 | takes `amount`, at least 0, of the player resource `name` from `player`, which must hold it |
| `teams` | read, gives a list of team names | every role | core | since 1.0 | the playing teams' names, the teams with slots |
| `teleport` | `(unit: Unit, position: Vector)` | every role | navigation | since 1.0 | puts `unit` at `position` at once, or at the nearest place it may stand |
| `units_tagged` | `(tag: text)`, `tag` a tag | every role | core | since 1.0 | the living units of a tag, by stable id |
| `win` | `(team: text)`, `team` a team | mode's own script | core | since 1.0 | ends the match, which `team` wins; a call once it ended does nothing |

## Damage `d`

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `action` | read, gives an action | combat | since 1.0 | the action that dealt it: an ability, or an attack's weapon; `()` for none |
| `amount` | read | combat | since 1.0 | raw in `calc_damage`, final in a hook |
| `hit` | read | combat | since 1.0 | how its projectile or area reached the target, `()` for damage none delivered |
| `is_action` | `(action: text)`, `action` an action | combat | since 1.0 | whether `action`, an action of the script's package, dealt it |
| `is_attack` | read | combat | since 1.0 | whether an attack dealt it |
| `is_extra` | read | combat | since 1.0 | whether `ctx.attack_hit` dealt it |
| `kind` | read, gives a damage kind | combat | since 1.0 | one of the mode's `[combat] damage_kinds` |
| `roll` | read | combat | since 1.0 | its attack's random number, at least 0 and less than 1, `()` for other damage |
| `source` | read | combat | since 1.0 | the unit that dealt it, `()` when gone or none |
| `target` | read | combat | since 1.0 | the unit it is dealt to |

## Heal `h`

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `action` | read, gives an action | combat | since 1.0 | the action that gave it, `()` for none |
| `amount` | read | combat | since 1.0 | before `calc_heal` and the heal scale |
| `is_action` | `(action: text)`, `action` an action | combat | since 1.0 | whether `action`, an action of the script's package, gave it |
| `is_leech` | read | combat | since 1.0 | whether its source's leech gave it |
| `source` | read | combat | since 1.0 | the unit that gave it, `()` when gone or none |
| `target` | read | combat | since 1.0 | the unit it heals |

## Hit `hit`

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `aimed` | read | abilities | since 1.0 | the unit the action aimed at, `()` with none |
| `delivery` | read | abilities | since 1.0 | the projectile or area unit that delivered it, `()` when at once or gone |
| `direction` | read | abilities | since 1.0 | the direction its delivery flew in, `()` for an area or a delivery that did not move |
| `distance` | read | abilities | since 1.0 | how far its delivery flew |
| `part` | read | hitboxes | planned | the body part a ray or a sweep struck, `()` with none |
| `position` | read | abilities | since 1.0 | where it hit, or where its delivery ended |

## Map, `ctx.map`

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `markers` | `(tag: text)`, `tag` a marker with tag | core | since 1.0 | the markers with `tag`, in the map's order |
| `paths` | read, gives a list of path names | navigation | since 1.0 | the paths' names |

## Marker, of `ctx.map.markers(tag)`

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `name` | read | core | since 1.0 | its name |
| `params` | read | core | since 1.0 | its params, by name |
| `position` | read | core | since 1.0 | its point, `()` for a region |
| `team` | read, gives a team | core | since 1.0 | its team's name, `()` with none |

## A match's result

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `is_draw` | read | core | since 1.0 | whether the match ended with no winner |
| `winner` | read, gives a team | core | since 1.0 | the team that won, `()` for a draw |

## Modifier `m`

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `carrier` | read | stats | since 1.0 | the unit that carries it |
| `source` | read | stats | since 1.0 | the unit that applied it, `()` when gone or none |
| `stacks` | written and read | stats | since 1.0 | its stacks, which a call may write and read back |
| `state` | read | stats | since 1.0 | its script state, by name, which a call may write and read back |

## Player

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `!=` | operator | core | since 1.0 | whether the two are two players |
| `==` | operator | core | since 1.0 | whether the two are one player |
| `is_out` | read | core | since 1.0 | whether it is out of play, as the last tick's end derived it |
| `resource` | `(name: text)`, `name` a player resource | core | since 1.0 | the amount of the player resource `name` it holds, with the call's changes |
| `seat` | read | core | since 1.0 | its seat's place, from 0 |
| `seated` | read | core | since 1.0 | whether a player or a bot held its slot as the match started |
| `state` | read | core | since 1.0 | its script state, by name, as the mode's `[player_state]` declares it, which a call may write and read back |
| `supply_cap` | read | production | since 1.0 | what its living, complete units give of supply, at most the mode's `max` |
| `supply_used` | read | production | since 1.0 | what its living units and queued trains use of its supply |
| `team` | read, gives a team | core | since 1.0 | its team's name |
| `units` | read | core | since 1.0 | the living units it owns, by stable id |

## Unit

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `!=` | operator | core | since 1.0 | whether the two are two units |
| `==` | operator | core | since 1.0 | whether the two are one unit |
| `attack_range` | read | combat | since 1.0 | its attack's range |
| `attack_target` | read | combat | since 1.0 | the unit its attack aims at, `()` with none |
| `can_see` | `(unit: Unit)` | vision | since 1.0 | whether its team sees the other unit |
| `has_modifier` | `(id: text)`, `id` a modifier | stats | since 1.0 | whether it carries the modifier of the script's package |
| `has_perk` | `(id)` | progression | planned | whether it has the perk `id` |
| `has_tag` | `(tag: text)`, `tag` a tag | core | since 1.0 | whether it has the tag, of its type or a modifier |
| `inside` | read | interaction | planned | the host it rides or garrisons in, `()` with none |
| `is_alive` | read | core | since 1.0 | whether it lives |
| `is_avatar` | read | core | since 1.0 | whether it is an avatar |
| `is_enemy_of` | `(unit: Unit)` | core | since 1.0 | whether its team may attack the other's, hostile or neutral |
| `level` | read | stats | since 1.0 | its level |
| `load` | read | production | since 1.0 | the amount it carries, 0 with none |
| `modifier` | `(id: text)` or `(id: text, source: Unit)`, `id` a modifier | stats | since 1.0 | its instance of the modifier `id` of the script's package, to read or remove: the one from `source`, or the first, from no source before any, then by source; `()` with none |
| `node_amount` | read | production | since 1.0 | the amount it holds as a node, `()` when it is no node |
| `owner` | read | core | since 1.0 | its player, `()` with none |
| `params` | read | core | since 1.0 | its unit type's params, unresolved |
| `passengers` | read | interaction | planned | the units inside it, by stable id |
| `path` | read, gives a path | navigation | since 1.0 | the name of the path it walks, `()` with none |
| `points` | read | progression | since 1.0 | its unspent points, which a unit with the `level` track has |
| `pool` | `(name: text)`, `name` a pool | stats | since 1.0 | the current amount of its pool `name` |
| `pool_max` | `(name: text)`, `name` a pool | stats | since 1.0 | the maximum of its pool `name` |
| `position` | read | core | since 1.0 | where it stands |
| `queue` | read, gives a list of unit type names | production | since 1.0 | the unit types its queue trains, first the one under way, none with no queue |
| `radius` | read | core | since 1.0 | its body's radius, 0 with no body, `()` for a box |
| `recent_attackers` | `(ms: ms)` | combat | since 1.0 | the living units that struck it within the last `ms`, rounded up to whole ticks |
| `site_progress` | read | production | since 1.0 | the share of its build's time its site reached, from 0 to 1, `()` when it is no site |
| `spawn_pos` | read | core | since 1.0 | where it spawned, where it respawns; `()` with none |
| `stat` | `(name: text)`, `name` a stat | stats | since 1.0 | its value of a stat the mode declares |
| `state` | read | core | since 1.0 | its script state, by name, which a call may write and read back |
| `team` | read, gives a team | core | since 1.0 | its team's name |
| `track_level` | `(track: text)`, `track` a track | progression | since 1.0 | its level on `track`, one of its unit type's |
| `unit_type` | read, gives a unit type | core | since 1.0 | its unit type's name |
| `xp` | `(track: text)`, `track` a track | progression | since 1.0 | its experience on `track`, one of its unit type's |

## Vector

| Name | Form | Capability | Status | What it is |
| --- | --- | --- | --- | --- |
| `!=` | operator | core | since 1.0 | whether the two are two vectors |
| `*` | operator | core | since 1.0 | the vector scaled by a number, on either side |
| `+` | operator | core | since 1.0 | the sum of two vectors |
| `-` | operator | core | since 1.0 | the difference of two vectors |
| `==` | operator | core | since 1.0 | whether the two are one vector |
| `direction_to` | `(position: Vector)` | core | since 1.0 | the unit vector towards `position` in the map's metric, `()` for the same point |
| `distance_to` | `(position: Vector)` | core | since 1.0 | the distance to `position` in the map's metric |
| `rotated_deg` | `(degrees: Num)` | core | since 1.0 | the vector turned by `degrees` about the vertical, counter-clockwise seen from above |
| `within` | `(position: Vector, radius: Num)` | core | since 1.0 | whether `position` is within `radius` in the map's metric, exactly: the reach rule between two points, which have no bodies |
| `x` | read | core | since 1.0 | its first component, across the ground |
| `y` | read | core | since 1.0 | its height |
| `z` | read | core | since 1.0 | its third component, across the ground |

## Engine enums

Each enum's module holds its members, and the function `named`, which gives the member a text names as data does; a member has `==`, `!=` and `to_string`, its name in data.

| Enum | Members |
| --- | --- |
| `Relation` | `Hostile`, `Neutral`, `Friendly` |
| `PathEnd` | `Start`, `End` |

## Hooks

| Hook | Role | Capability | Status |
| --- | --- | --- | --- |
| `on_channel_tick(ctx, caster, target)` | action | abilities | since 1.0 |
| `on_end(ctx, caster, hit)` | action | core | since 1.0 |
| `on_hit(ctx, caster, target, hit)` | action | core | since 1.0 |
| `on_interrupt(ctx, caster, target)` | action | abilities | since 1.0 |
| `on_resolve(ctx, caster, target)` | action | abilities | since 1.0 |
| `on_attack(ctx, m, target)` | modifier | combat | since 1.0 |
| `on_attack_hit(ctx, m, d)` | modifier | combat | since 1.0 |
| `on_damage_taken(ctx, m, d)` | modifier | combat | since 1.0 |
| `on_interval(ctx, m)` | modifier | stats | since 1.0 |
| `on_kill(ctx, m, victim)` | modifier | combat | since 1.0 |
| `on_takedown(ctx, m, victim)` | modifier | combat | since 1.0 |
| `calc_damage(ctx, d)` | mode | combat | since 1.0 |
| `calc_heal(ctx, h)` | mode | combat | since 1.0 |
| `on_generate(ctx, region)` | mode | core | planned |
| `on_input(ctx, player, name, value)` | mode | core | since 1.0 |
| `on_level_up(ctx, unit, track, level)` | mode | progression | since 1.0 |
| `on_match_start(ctx)` | mode | core | since 1.0 |
| `on_path_end(ctx, unit, path)` | mode | navigation | since 1.0 |
| `on_player_join(ctx, player)` | mode | core | since 1.0 |
| `on_player_leave(ctx, player)` | mode | core | since 1.0 |
| `on_quest_stage(ctx, player, quest, stage)` | mode | quests | planned |
| `on_site_complete(ctx, unit)` | mode | production | since 1.0 |
| `on_system(ctx, units)` | mode | core | planned |
| `on_timer(ctx, name, data)` | mode | core | since 1.0 |
| `on_unit_death(ctx, unit, killer, assisters)` | mode | core | since 1.0 |
| `on_unit_trained(ctx, unit, producer)` | mode | production | since 1.0 |
| `on_think(ctx, unit)` | AI | orders | since 1.0 |

## Tag properties

| Property | Status |
| --- | --- |
| `blocks = ["move"]` | since 1.0 |
| `blocks = ["use"]` | since 1.0 |
| `immune = [tags]` | since 1.0 |
| `blocks = ["attack"]` | since 1.0 |
| `blocks = ["target"]` | since 1.0 |
| `blocks = ["damage"]` | since 1.0 |
| `hidden = true` | since 1.0 |
| `detects = true` | since 1.0 |
| `blocks = ["cast"]` | since 1.0 |
| `blocks = ["produce"]` | since 1.0 |

## Data fields

### A unit type, `[units.<id>]`

| Field | Capability | Status |
| --- | --- | --- |
| `tags` | core | since 1.0 |
| `params` | core | since 1.0 |
| `state` | core | since 1.0 |
| `collision` | core | since 1.0 |
| `bounty` | core | since 1.0 |
| `slots` | core | since 1.0 |
| `pools` | stats | since 1.0 |
| `stats` | stats | since 1.0 |
| `passive` | stats | since 1.0 |
| `tracks` | progression | since 1.0 |
| `experience` | progression | since 1.0 |
| `combat` | combat | since 1.0 |
| `turret` | combat | since 1.0 |
| `vision` | vision | since 1.0 |
| `projectile` | projectiles | since 1.0 |
| `area` | areas | since 1.0 |
| `production` | production | since 1.0 |
| `supply` | production | since 1.0 |
| `node` | production | since 1.0 |
| `drop_off` | production | since 1.0 |
| `inventory` | items | since 1.0 |
| `shop` | items | since 1.0 |
| `use` | interaction | since 1.0 |
| `uses` | interaction | since 1.0 |
| `orders` | orders | since 1.0 |
| `acquire` | orders | since 1.0 |

### `data/mode.toml`

| Field | Capability | Status |
| --- | --- | --- |
| `tags` | core | since 1.0 |
| `victory` | core | since 1.0 |
| `bounty` | core | since 1.0 |
| `resources` | core | since 1.0 |
| `actions` | core | since 1.0 |
| `slots` | core | since 1.0 |
| `modifiers` | stats | since 1.0 |
| `stats` | stats | since 1.0 |
| `pools` | stats | since 1.0 |
| `tracks` | progression | since 1.0 |
| `experience` | progression | since 1.0 |
| `combat` | combat | since 1.0 |
| `navigation` | navigation | since 1.0 |
| `supply` | production | since 1.0 |
| `items` | items | since 1.0 |
| `name` | core | since 1.0 |
| `choices` | core | since 1.0 |
| `inputs` | core | since 1.0 |
| `timers` | core | since 1.0 |
| `marker_params` | core | since 1.0 |
| `state_version` | core | planned |
| `state` | core | since 1.0 |
| `player_state` | core | since 1.0 |
| `params` | core | since 1.0 |
| `options` | core | since 1.0 |
| `teams` | core | since 1.0 |
| `relations` | core | since 1.0 |
| `players` | core | since 1.0 |
| `saves` | core | since 1.0 |
| `visibility` | core | since 1.0 |
| `scripts` | core | since 1.0 |
| `defaults` | core | since 1.0 |

### A unit type's `collision`

| Field | Capability | Status |
| --- | --- | --- |
| `radius` | core | since 1.0 |
| `box` | core | since 1.0 |
| `height` | core | since 1.0 |
| `layer` | core | since 1.0 |

### A tag's properties, `[tags.<name>]`

| Field | Capability | Status |
| --- | --- | --- |
| `blocks` | core | since 1.0 |
| `hidden` | core | since 1.0 |
| `detects` | core | since 1.0 |
| `immune` | core | since 1.0 |

### An effect of an action's `on_resolve`, `on_hit` or `on_end`

| Field | Capability | Status |
| --- | --- | --- |
| `spawn` | core | since 1.0 |
| `to` | core | since 1.0 |
| `loot` | core | planned |
| `noise` | core | planned |
| `modifier` | stats | since 1.0 |
| `purge` | stats | since 1.0 |
| `restore` | stats | since 1.0 |
| `xp` | progression | since 1.0 |
| `damage` | combat | since 1.0 |
| `heal` | combat | since 1.0 |
| `move` | navigation | since 1.0 |
| `land` | areas | since 1.0 |

### An action, `[actions.<id>]`

| Field | Capability | Status |
| --- | --- | --- |
| `name` | core | since 1.0 |
| `kind` | core | since 1.0 |
| `script` | core | since 1.0 |
| `targeting` | core | since 1.0 |
| `range` | core | since 1.0 |
| `min_range` | core | since 1.0 |
| `cooldown_ms` | core | since 1.0 |
| `cost` | core | since 1.0 |
| `windup_ms` | core | since 1.0 |
| `clamp_to_range` | core | since 1.0 |
| `toggle` | core | since 1.0 |
| `channel` | core | since 1.0 |
| `hold` | core | since 1.0 |
| `charges` | core | since 1.0 |
| `charging` | core | since 1.0 |
| `passive_modifier` | core | since 1.0 |
| `passive_while_ready` | core | since 1.0 |
| `delivery` | core | since 1.0 |
| `params` | core | since 1.0 |
| `on_resolve` | core | since 1.0 |
| `on_hit` | core | since 1.0 |
| `on_end` | core | since 1.0 |
| `rate` | combat | since 1.0 |
| `rate_bonus` | combat | since 1.0 |
| `damage` | combat | since 1.0 |
| `damage_kind` | combat | since 1.0 |
| `amount` | combat | since 1.0 |
| `period_ms` | combat | since 1.0 |
| `clip` | combat | since 1.0 |
| `splash` | combat | since 1.0 |
| `scatter` | combat | since 1.0 |
| `pitch_deg` | combat | since 1.0 |
| `speed` | combat | since 1.0 |
| `streak` | combat | since 1.0 |
| `unit_type` | production | since 1.0 |
| `requires` | production | since 1.0 |
| `construct` | production | since 1.0 |
| `start` | production | since 1.0 |
| `start_life` | production | since 1.0 |
| `cancel_refund` | production | since 1.0 |
| `placement` | production | since 1.0 |
| `resource` | production | since 1.0 |
| `take` | production | since 1.0 |
| `unit` | production | since 1.0 |
| `bounce` | production | since 1.0 |
| `deliver_ms` | production | since 1.0 |

### A modifier, `[modifiers.<id>]`

| Field | Capability | Status |
| --- | --- | --- |
| `script` | stats | since 1.0 |
| `duration_ms` | stats | since 1.0 |
| `interval_ms` | stats | since 1.0 |
| `interval_revival` | stats | since 1.0 |
| `stacks_expire_ms` | stats | since 1.0 |
| `reapply` | stats | since 1.0 |
| `max_stacks` | stats | since 1.0 |
| `stats` | stats | since 1.0 |
| `tags` | stats | since 1.0 |
| `shield` | stats | since 1.0 |
| `aura` | stats | since 1.0 |
| `affects` | stats | since 1.0 |
| `params` | stats | since 1.0 |
| `state` | stats | since 1.0 |

### A modifier's `aura`

| Field | Capability | Status |
| --- | --- | --- |
| `radius` | stats | since 1.0 |
| `affects` | stats | since 1.0 |
| `modifier` | stats | since 1.0 |

### A track, `[tracks.<name>]`

| Field | Capability | Status |
| --- | --- | --- |
| `levels` | progression | since 1.0 |
| `level` | progression | since 1.0 |

### An action's `kind`

| Field | Capability | Status |
| --- | --- | --- |
| `attack` | combat | since 1.0 |
| `cast` | abilities | since 1.0 |
| `train` | production | since 1.0 |
| `build` | production | since 1.0 |
| `gather` | production | since 1.0 |
| `use` | interaction | since 1.0 |

### The mode's `[combat]`

| Field | Capability | Status |
| --- | --- | --- |
| `damage_kinds` | combat | since 1.0 |
| `assist_window_ms` | combat | since 1.0 |
| `life` | combat | since 1.0 |
| `leech` | combat | since 1.0 |
| `heal_scale` | combat | since 1.0 |
| `armors` | combat | since 1.0 |

### The mode's `[combat] leech`

| Field | Capability | Status |
| --- | --- | --- |
| `attack` | combat | since 1.0 |
| `other` | combat | since 1.0 |

### A unit type's `combat`

| Field | Capability | Status |
| --- | --- | --- |
| `on_death` | combat | since 1.0 |
| `armor` | combat | since 1.0 |

### A unit type's `turret`

| Field | Capability | Status |
| --- | --- | --- |
| `turn_rate` | combat | since 1.0 |
| `tolerance_deg` | combat | since 1.0 |
| `slot` | combat | since 1.0 |

### The mode's `[navigation]`

| Field | Capability | Status |
| --- | --- | --- |
| `layers` | navigation | since 1.0 |
| `spawn_tags` | navigation | since 1.0 |

### A unit type's `vision`

| Field | Capability | Status |
| --- | --- | --- |
| `sight_range` | vision | since 1.0 |

### An action's `delivery`

| Field | Capability | Status |
| --- | --- | --- |
| `projectile` | projectiles | since 1.0 |
| `count` | projectiles | since 1.0 |
| `spread_deg` | projectiles | since 1.0 |
| `area` | areas | since 1.0 |

### A unit type's `projectile`

| Field | Capability | Status |
| --- | --- | --- |
| `speed` | projectiles | since 1.0 |
| `width` | projectiles | since 1.0 |
| `range` | projectiles | since 1.0 |
| `homing` | projectiles | since 1.0 |
| `stop_on_hit` | projectiles | since 1.0 |
| `once_per_cast` | projectiles | since 1.0 |
| `hits` | projectiles | since 1.0 |
| `gravity` | projectiles | planned |
| `curve` | projectiles | since 1.0 |

### A unit type's `area`

| Field | Capability | Status |
| --- | --- | --- |
| `radius` | areas | since 1.0 |
| `delay_ms` | areas | since 1.0 |
| `duration_ms` | areas | since 1.0 |
| `affects` | areas | since 1.0 |
| `inside` | areas | since 1.0 |

### An area's `inside`

| Field | Capability | Status |
| --- | --- | --- |
| `self` | areas | since 1.0 |
| `allies` | areas | since 1.0 |
| `enemies` | areas | since 1.0 |

### A unit type's `production`

| Field | Capability | Status |
| --- | --- | --- |
| `queue` | production | since 1.0 |
| `exit` | production | since 1.0 |
| `rally` | production | since 1.0 |
| `gathers` | production | since 1.0 |
| `free` | production | since 1.0 |

### A unit type's `supply`

| Field | Capability | Status |
| --- | --- | --- |
| `cost` | production | since 1.0 |
| `provides` | production | since 1.0 |

### The mode's `[supply]`

| Field | Capability | Status |
| --- | --- | --- |
| `max` | production | since 1.0 |

### A train's `requires`

| Field | Capability | Status |
| --- | --- | --- |
| `units` | production | since 1.0 |
| `modifiers` | production | since 1.0 |

### A unit type's `node`

| Field | Capability | Status |
| --- | --- | --- |
| `resource` | production | since 1.0 |
| `amount` | production | since 1.0 |
| `empty` | production | since 1.0 |

### A unit type's `drop_off`

| Field | Capability | Status |
| --- | --- | --- |
| `resources` | production | since 1.0 |

### An item type, `[items.<id>]`

| Field | Capability | Status |
| --- | --- | --- |
| `name` | items | since 1.0 |
| `cost` | items | since 1.0 |
| `components` | items | since 1.0 |
| `stack` | items | since 1.0 |
| `uses` | items | since 1.0 |
| `modifiers` | items | since 1.0 |
| `action` | items | since 1.0 |

### A unit type's `inventory`

| Field | Capability | Status |
| --- | --- | --- |
| `slots` | items | since 1.0 |
| `kind` | items | since 1.0 |

### A unit type's `shop`

| Field | Capability | Status |
| --- | --- | --- |
| `items` | items | since 1.0 |
| `resource` | items | since 1.0 |
| `reach` | items | since 1.0 |
| `buyers` | items | since 1.0 |
| `sell_share` | items | since 1.0 |

### A unit type's `use`

| Field | Capability | Status |
| --- | --- | --- |
| `action` | interaction | since 1.0 |
| `users` | interaction | since 1.0 |

### A unit type's `uses`

| Field | Capability | Status |
| --- | --- | --- |
| `kind` | interaction | since 1.0 |

### A unit type's `orders`

| Field | Capability | Status |
| --- | --- | --- |
| `ai` | orders | since 1.0 |
| `think_ms` | orders | since 1.0 |

### A choice, `[choices.<name>]`

| Field | Capability | Status |
| --- | --- | --- |
| `name` | core | since 1.0 |
| `offers` | core | since 1.0 |
| `unique` | core | since 1.0 |
| `count` | core | since 1.0 |
| `slot` | core | since 1.0 |
| `values` | core | since 1.0 |

### A slot kind, `[[slots]]`

| Field | Capability | Status |
| --- | --- | --- |
| `name` | core | since 1.0 |
| `ranks` | core | since 1.0 |
| `levels` | core | since 1.0 |

### A team, `[teams.<team>]`

| Field | Capability | Status |
| --- | --- | --- |
| `name` | core | since 1.0 |

### A pair of teams, `[[relations]]`

| Field | Capability | Status |
| --- | --- | --- |
| `teams` | core | since 1.0 |
| `relation` | core | since 1.0 |
| `vision` | core | since 1.0 |

### The mode's `[players]`

| Field | Capability | Status |
| --- | --- | --- |
| `late_join` | core | since 1.0 |
| `bot_takeover` | core | since 1.0 |
| `leaver` | core | since 1.0 |
| `control` | core | since 1.0 |

### The mode's `[saves]`

| Field | Capability | Status |
| --- | --- | --- |
| `by` | core | since 1.0 |
| `autosave_ms` | core | since 1.0 |

### An option, `[options.<name>]`

| Field | Capability | Status |
| --- | --- | --- |
| `values` | core | since 1.0 |
| `default` | core | since 1.0 |

### The mode's `[victory]`

| Field | Capability | Status |
| --- | --- | --- |
| `counts` | core | since 1.0 |
| `defeat` | core | since 1.0 |

### The mode's `[bounty]`

| Field | Capability | Status |
| --- | --- | --- |
| `killer` | core | since 1.0 |
| `victim` | core | since 1.0 |
| `share` | core | since 1.0 |

### A unit type's `bounty`

| Field | Capability | Status |
| --- | --- | --- |
| `resource` | core | since 1.0 |
| `amount` | core | since 1.0 |
| `cost` | core | since 1.0 |
