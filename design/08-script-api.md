# Campfire — Script API

Draft, derived from the reference packages in `source/packages/moba/`: six heroes, the player spells and the 3v3 mode. Every name below is used there; a name the packages do not need is not here. Nothing runs yet.

Each call, handle field and hook belongs to the core or to one capability ([Capabilities](04-capabilities/00-overview.md)); a package gets those of the capabilities it declares, and the load checks refuse the rest. The tables below name each one's capability.

## Rules

- **Data first.** What the engine can do from data, it does: held modifiers, charges, charged casts, projectiles, areas, auras, shields. Scripts describe only what is special about an effect.
- **Hooks by name.** The engine calls a script function by its hook name. Hook names differ between roles (ability, modifier, mode, AI), so one file can serve an ability and the modifiers it applies.
- **Handles read, `ctx` changes.** Handles are read-only, except `.state` and a modifier's `.stacks`, which a call may write and read back. Every other change goes through `ctx` and is queued: the call is all-or-nothing ([Scripting](02-engine-core.md#scripting)).
- **Default source.** `ctx` knows the acting unit: the caster, a projectile's or area's source, or a modifier's source. Damage, heals, modifiers, projectiles and areas come from it unless a call names another.
- **New entities at once.** A handle to an entity created in a call is usable in that call, including `.state`; the entity appears when the call commits.
- **Event order.** A call's effects apply in call order. The hooks they trigger are queued first-in first-out and run after all of the call's effects. A hook of a modifier that is gone by then is skipped. A chain longer than an engine constant fails with `script_error`.
- **Dead and despawned units.** A handle keeps returning the unit's last values, and `alive` is false. Effects on a dead unit do nothing, except `respawn`. A modifier whose source despawned keeps that source's last stats.
- **Values are live.** `ctx.p` resolves when it is read, with the current rank and the current stats of the source.
- **Pure hooks.** `calc_damage` returns a value; its `ctx` refuses effects.
- **Handles last one call.** A call sees the units as its stage began; a handle is valid within the call. `unit.target` is `()` when the unit has no target, or its target is gone. `unit.recent_attackers(ms)` lists the living units that struck it within the last `ms`, rounded up to whole ticks.
- **Queries.** Lists are sorted by stable id; `nearest_visible` sorts by distance, then id. `find`, `find_visible` and `nearest_visible` never return dead or untargetable units; `heroes` and `units_tagged` return the dead too, so a mode sees a structure that fell. `find` includes units hidden from the caller's team, for effects on an area; `find_visible` and `nearest_visible` return only what that team sees, for choosing a target.
- **Numbers.** [Game Scripting](03-game-scripting.md#numbers). `num(i)` makes a `Num` from an integer. `Num` has `min`, `max`, `clamp`, `round`, `floor`, `ceil`; `round` and the others return integers.
- **Randomness** comes only from `ctx.chance` and `ctx.pick`, both from the secret stream. Crits are rolled by `combat`.
- **Time** is in milliseconds, rounded up to whole ticks.
- **Rhai names.** Rhai's reserved words cannot be names, including `spawn` and `match`.

## Filters

One set of words selects units everywhere: `targeting`, projectile `hits`, area `affects`, aura `affects`, and every query. A filter is a relation, `enemies`, `allies` or `all`, and an optional tag after a colon: `enemies:hero`, `allies:hero`, `enemies:creep`. Allies include the unit itself; units of different teams are enemies, so the `neutral` team is an enemy of every other. Heroes carry the tag `hero`.

## Data files

Arrays in capability fields and in the params of an ability are per rank: 5 entries for a basic ability, 3 for the ultimate. Anywhere else an array is a list.

**Param values** (`[params]`, read as `ctx.p.<name>`; an unknown name is an error):

| Form | Example | Value |
| --- | --- | --- |
| Integer | `slow_ms = 2000` | the integer |
| Decimal string | `radius = "3.0"` | a `Num` |
| Array | `slow = ["0.15", "0.20", "0.25", "0.30", "0.35"]` | the entry for the current rank |
| Scaling table | `{ base = [80, 120, 160, 200, 240], ap = "0.65" }` | `base + per_level × level + Σ ratio × stat` of the source |

Scaling keys: `base`, `per_level`, and a stat ratio each for `ad`, `bonus_ad`, `ap`, `max_health`, `bonus_health`, `armor`, `magic_resist`. A number field anywhere in data may be `{ param = "<name>" }`, resolved in the same way. An ability's `range`, `cooldown_ms`, `cost` and `cast_time_ms` resolve when the ability loads, one value for each rank, so a param they name is a value or a per-rank array, not a scaling table.

**Hero** (`data/hero.toml`): `name`, `role`, `resource` (`mana` or `energy`), `passive` (a modifier id), `slots` (four ability ids; the last is the ultimate), `[combat.attack]` (`range`, `windup_ms`, `projectile_speed`; no speed means melee), `[stats]` (`stat = { base, per_level }`; the value at level `n` is `base + per_level × (n − 1)`), `[abilities.<id>]`, `[modifiers.<id>]`.

**Ability:**

| Field | Meaning |
| --- | --- |
| `script` | Script file, if the ability needs one |
| `targeting` | `none`, `point`, `direction`, or a filter for a unit target |
| `range` | Meters, or `"global"` |
| `cooldown_ms`, `cost`, `cast_time_ms` | Cost is in the hero's resource |
| `clamp_to_range` | A target beyond range is moved in, instead of the caster walking |
| `toggle` | `{ cost_per_attack }` or `{ cost_per_second }`; `abilities` turns it off at zero resource and at death |
| `channel` | `{ duration_ms, tick_ms }`; starts after `on_cast`; calls `on_channel_tick` |
| `hold` | Modifier `abilities` holds while the toggle is on or the channel runs |
| `charges` | `{ max, recharge_ms }` |
| `charge` | `{ max_ms }`: a charged cast. `on_cast` runs at release, with `ctx.charge` from 0 to 1 and `ctx.origin` and the target as they were when the charge began |
| `passive_modifier` | Modifier held while the ability has a rank; with `passive_while_ready`, only while it is off cooldown |
| `projectile` | `speed`, `width`, `range` (default: the ability's), `stop_on_hit`, `once_per_cast`, `hits`, `sight_radius`, `collide` |
| `area` | `radius`, `delay_ms`, `duration_ms`, `affects`, and `inside = { self, allies, enemies }`: modifiers held on units while they are inside |
| `[params]`, `[projectile_state]` | Script values; the state of each projectile the ability fires |

**Modifier:** `script`, `duration_ms` (absent: until removed), `interval_ms`, `stacks_expire_ms`, `reapply` (`refresh` by default, `stack`, `ignore`), `stats` (per stack), `states`, `shield` (the modifier ends when the shield is spent), `aura` (`{ radius, affects, modifier }`), `[params]`, `[state]`. A unit holds at most one instance of each modifier id from each source.

**Manifest** (`manifest.toml`): `name`, `version`, `engine` (the release tag it targets) and `kind`: `mode`, `hero` or `spells`. A mode's also has `capabilities` (none twice, not `mode`, and each with the ones it builds on: `projectiles` and `abilities` on `combat`, `orders` on `combat` and `navigation`), `tick_hz = { min, max, default }` (all positive, `min ≤ default ≤ max`), `teams` (each `{ name, slots }`; their slots in order make the player slots), `backends`, `max_move_speed` (m/s, positive), `script_limits = { per_call, player, think, mode }` (each pool holds a whole call; each player slot has a pool of the size `player`), and `[dependencies]`, each named by its package's name, and in the workspace given by path. A hero's id is its package's name.

**Mode** (`data/mode.toml`): `script`, `assist_window_ms`, `[inputs]` (name and type of each player input: `string`, `string_list`), `[state]`, `[params]` (values, or lists whose entries are values or strings, such as unit types), `[modifiers]`. An input that does not match its type never reaches the script. **Units** (`data/units.toml`): `[units.<id>]` with `tags`, `params`, and a section for each capability the unit type uses ([Unit types](04-capabilities/00-overview.md#unit-types)): `stats`, `combat` (`attack = { range, windup_ms, projectile_speed }`, and `on_death`: `stay` or `despawn`, the default), `orders` (`ai`, `think_ms`), `vision` (`true_sight`). Health, attack damage, attack speed and move speed are stats: an attack starts at most `attack_speed` times a second, its period the tick rate over that rounded up.

**Map** (`map/map.toml`): `[[lanes]]` (`name`, `points` from the first team's end to the second's), `[spawns]` (each playing team's hero spawn), `[[structures]]` (`unit_type`, `team`, `lane` if it guards one, `pos`: they stand from the start), `[[neutral_spawns]]` (`unit_type`, `pos`, as `ctx.map.neutral_spawns` lists them). A point is `[x, z]` in meters on the ground plane.

**State types:** `int`, `num`, `bool`, `string`, `entity`, `entity_list`, `pos`, `vec`; each with a `default` and, for mode state, `sync`.

**Stats:**

| Stats | Combine | Limits |
| --- | --- | --- |
| `health`, `health_regen`, `resource`, `resource_regen`, `attack_damage`, `ability_power`, `armor`, `magic_resist`, `armor_pen`, `magic_pen`, `move_speed`, `physical_block` | Sum | — |
| `attack_damage_pct`, `ability_power_pct`, `attack_speed_pct`, `move_speed_pct`, `healing_received_pct`, `damage_dealt_pct` | Sum | ≥ −1 |
| `attack_speed` | Sum, then × (1 + `attack_speed_pct`) | ≤ 2.5 attacks per second |
| `crit_chance`, `armor_pen_pct`, `magic_pen_pct`, `life_steal`, `spell_vamp` | Sum | 0 to 1 |
| `cooldown_reduction` | Sum | 0 to 0.4 |
| `slow` | Strongest only | 0 to 0.99; `slow_immune` ignores it |

Until modifiers come, a unit's stats are its type's at level 1. Move speed is `move_speed × (1 + move_speed_pct) × (1 − slow)`, at least an engine floor and at most the mode's `max_move_speed`. Every homing projectile flies faster than that cap, which the package load checks, so it catches its target within launch distance ÷ (projectile speed − cap). `life_steal` heals the source for attack damage dealt and `spell_vamp` for ability damage; `healing_received_pct` scales every heal.

**States:** `stunned`, `rooted`, `silenced`, `disarmed`, `airborne`, `stealthed`, `untargetable`, `slow_immune`.

## Handles

A unit handle has the fields of the capabilities its type uses; reading a field it lacks is a script error.

| Handle | Reads |
| --- | --- |
| Unit | `pos`, `team`, `owner`, `level`, `alive`, `is_hero`, `target`, `attack_range`, `health`, `max_health`, `stat(name)`, `params` (the unit type's, unresolved), `spawn_pos`, `lane`, `unit_type`, `has_tag(tag)`, `has_modifier(id)`, `is_enemy_of(unit)`, `can_see(unit)`, `recent_attackers(ms)` |
| Modifier `m` | `carrier`, `source`, `stacks` (writable), `state` (writable) |
| Projectile | `source`, `pos`, `distance` flown, `state` (writable) |
| Area | `source`, `pos` |
| Damage `d` | `source`, `target`, `amount`, `kind` (`physical`, `magic`, `true`), `attack`, `crit`, `extra` (an attack from `ctx.attack_hit`), `ability` (`""` when none) |
| Position, vector | `distance_to`, `within(pos, radius)` (on the ground plane and exact, as every range: the test for reach), `direction_to`, `rotated_deg`, `+`, `-`, `*` |

## `ctx`

| Group | Capability | Calls |
| --- | --- | --- |
| Values | core; `abilities` for `range`, `charge`, `origin`; `navigation` for `map` | `p.<name>`, `range`, `charge`, `origin`, `state` (mode), `map` (`lanes`, `neutral_spawns`), `teams` (playing teams) |
| Queries | core; `vision` for the visible ones | `find(of, pos, radius, filter)`, `find_visible(of, pos, radius, filter)`, `nearest_visible(of, radius, filter)`, `heroes()`, `heroes(team)`, `units_tagged(tag)`, `enemy_team(team)`, `hero_available(player, id)` |
| Random | core | `chance(p)`, `pick(list)` |
| Combat | `combat` | `damage(target, amount, kind)`, `heal(unit, amount)`, `restore(unit, amount)` (the unit's resource), `attack_hit(target)` |
| Modifiers | `stats` | `add_modifier(unit, id)` or `(unit, id, duration_ms)`, returns the modifier; `remove(handle)` removes a modifier, projectile or area |
| Crowd control | `stats` | `stun(unit, ms)`, `slow(unit, fraction, ms)`, `knock_up(unit, ms)`, `knock_back(unit, from, distance, ms)` |
| Movement | `navigation` | `dash(unit, to, speed)`, `teleport(unit, pos)` |
| Projectiles | `projectiles` | `projectile(from, to)` or `(from, to, overrides)`: the ability's `projectile` data; `to` a direction flies a line, a unit homes, a position flies to it; `from` a unit or a position |
| Areas | `areas` | `area(pos)`: the ability's `area` data |
| Vision | `vision` | `reveal(pos, radius, ms)` for the source's team |
| Abilities | `abilities` | `reduce_cooldown(unit, id, ms)`, `reduce_cooldowns(unit, fraction)` (basic abilities), `add_charge(unit, id)` |
| Progress | core; `stats` for experience | `add_resource(player, name, amount)`, `add_xp(hero, amount)` |
| Orders (AI) | `orders` | `order_attack(unit, target)`, `order_move(unit, pos)`, `order_follow_lane(unit)`, `order_reset(unit)` (walk home, heal, drop target) |
| Mode | core; `combat` for `respawn` | `timer(name, ms, repeat, data)`, `end(result)`, `spawn_heroes()`, `spawn_unit(type, team, pos)`, `spawn_wave(team, lane, types)`, `respawn(unit, ms)`, `choose_hero(player, id)`, `choose_spells(player, ids)` |

`ctx.p` reads, in an ability, its params; in a modifier, the modifier's params and then those of the ability that applied it; in a mode or AI script, the mode's params.

**Mode calls.** The map's structures spawn, then `on_match_start` runs, before the first tick. A timer counts from its call, in ticks rounded up, at least one, and fires in the Mode stage at the end of a tick: set before the first tick, 1 ms fires at the end of tick 0. `ctx.teams` names the playing teams; neutral units spawn on `neutral`, and `enemy_team` needs a mode of two playing teams. `spawn_wave` spawns its units in order at the team's end of the lane, the first team at its start and the second at its end, walking it. `choose_hero` takes a hero no other player chose; `spawn_heroes` spawns each chosen hero not yet spawned, in slot order, at its team's spawn, its abilities unlearned and its player's spells at rank 1. A player's mode input is a command of the `mode` owner: its name, then its value in its declared type, in postcard.

## Hooks

| Role | Capability | Hooks |
| --- | --- | --- |
| Ability | `abilities`, `projectiles`, `areas` | `on_cast(ctx, caster, target)` (target: a unit, a position or `()`; a script that serves only the ability's modifiers has none, and a cast then runs no script), `on_channel_tick(ctx, caster)`, `on_dash_end(ctx, unit, target)`, `on_projectile_hit(ctx, proj, target)`, `on_projectile_end(ctx, proj)`, `on_area_trigger(ctx, area, units)` |
| Modifier | `stats`, `combat` for the carrier's events | `on_interval(ctx, m)`, and the carrier's events: `on_attack(ctx, m, target)`, `on_attack_hit(ctx, m, d)`, `on_damage_taken(ctx, m, d)`, `on_kill(ctx, m, victim)`, `on_takedown(ctx, m, victim)` |
| Mode | core; `combat` for `on_unit_died` and `calc_damage` | `on_match_start(ctx)`, `on_mode_input(ctx, player, name, value)`, `on_timer(ctx, name, data)`, `on_player_join(ctx, player)`, `on_player_leave(ctx, player)`, `on_unit_died(ctx, unit, killer, assisters)`, `calc_damage(ctx, d)` |
| AI | `orders` | `think(ctx, unit)` |

A hero passive is a modifier the hero always carries. `combat` finds takedown participants once, with the mode's `assist_window_ms`: they receive `on_takedown`, and the mode receives them as `assisters`.

## Checks at package load

A package loads only when all of these pass:

- Every data file matches its schema, every per-rank array has one entry for each rank, and every capability field of an ability (`range`, `cooldown_ms`, `cost`, `cast_time_ms`) holds at every rank.
- The mode's unit types declare at most 64 tags together, and no hero has the name of one of them.
- Every script is referenced by data. Every function named like a hook, a hook's name or any name that starts with `on_`, is a hook of a role the script serves, with the hook's parameters, so a misspelled hook is an error, not a hook that never runs.
- Every modifier id, `ctx.p` name, `{ param }` reference, stat, state, filter and damage kind that a script or data file names exists. Scripts are read with `AST::walk`, from Rhai's `internals` feature.
- Every `ctx` call is one this API defines, of a capability the mode declares, for the script's role.
- Every value of `ctx` is a variable named `ctx`, so the checks see all its uses: every hook's first parameter is named `ctx`; `ctx` is used only as `ctx.<name>` or as a whole argument of a call, not of an operator or a function pointer's `call` or `curry`; a function of the script that receives it names that parameter `ctx`; and no `let`, `const` or `for` binds a new `ctx`.
- Every capability a package's data or scripts use is declared, and each builds on the ones it needs. A capability the release does not run yet loads: its data is checked, and a call to it fails at run time.
- The manifest's capabilities, tick rates, pools and move speed cap hold, where the manifest is read. Every package targets this release, and every projectile flies faster than the cap.
- The map and the teams name only what the mode has: every structure's and neutral spawn's unit type, team and lane; a hero spawn for each playing team; no team named `neutral`, and no two teams, lanes, slots of a hero or spells of the mode's spells packages alike.

## Found while writing the scripts

1. Passives are permanent modifiers, and carrier events go to modifier scripts: one mechanism for passives, buffs, marks and, later, items.
2. Toggles, channels, charged casts, charges and granted passives are data. `abilities` holds their modifiers, so no script can leave one behind.
3. Projectiles and areas are data; areas hold modifiers on the units inside, which needs no tick script.
4. Values must be live: resolving them once when a modifier is applied froze passives at their level-1 values.
5. Stats need a rule for how they combine and limits; slows as negative move speed added up without limit.
6. Modifier identity is (id, source), with a rule for applying one again.
7. Handles to dead or despawned units, and hooks of removed modifiers, need defined behavior.
8. `combat`, not the mode, decides who assisted.
9. Player inputs are untrusted and typed in data.
10. Rhai's default limits differ between debug and release builds, so the engine sets every limit itself ([Scripting](02-engine-core.md#scripting)).
11. `spawn` is reserved in Rhai.
12. A package in the workspace names its dependencies by path; the built package names them by fingerprint.

## Open questions

- [ ] Camp monsters respawn one by one; whole camps may be better.
- [ ] Kill streaks and assist rules beyond an even gold split.
- [ ] Items: the same modifier and ability model, not yet written.
