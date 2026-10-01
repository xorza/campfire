# Campfire — Script API

Derived from the reference packages in `source/packages/moba/`: six heroes, the player spells and the 3v3 mode. Every name is used there; a name the packages do not need is not here. The [reference](08-script-api-reference.md), which the registry writes, lists every name with its roles, capability and whether the release runs it.

Each call, handle field and hook belongs to the core or to one capability ([Capabilities](04-capabilities/00-overview.md)); a package gets those of the capabilities it declares, and the load checks refuse the rest. The reference names each one's capability.

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
- **Queries.** Lists are sorted by stable id; `nearest_visible` sorts by distance, then id. `find`, `find_visible` and `nearest_visible` never return dead or untargetable units; `heroes` and `units_tagged` return the dead too, so a mode sees a structure that fell. `find` includes units hidden from the caller's team, for effects on an area; `find_visible` and `nearest_visible` return only what that team sees, for choosing a target, and `unit.can_see(other)` tells whether the unit's team sees `other`. What a team sees is what the Vision stage of the last tick found; without `vision`, every team sees every unit.
- **Numbers.** [Game Scripting](03-game-scripting.md#numbers). `num(i)` makes a `Num` from an integer. `Num` has `min`, `max`, `clamp`, `round`, `floor`, `ceil`; `round` and the others return integers. The operators, the comparisons, `min` and `max` take an integer on either side.
- **Randomness** comes only from `ctx.chance` and `ctx.pick`, both from the secret stream. Crits are rolled by `combat`.
- **Time** is in milliseconds, rounded up to whole ticks.
- **Rhai names.** Rhai's reserved words cannot be names, including `spawn` and `match`.

## One source

The registry replaced the hand-kept lists the load check read (a table of `ctx` names beside three `ctx` types that each registered their own), which drifted from what the engine runs: a name loaded that no code ran, or that one role ran and another did not.

Established engines keep one source for what scripts may use, and derive everything else from it. Godot's `ClassDB` is filled by the same `bind_method` call that binds the code; the script analyzer and the editor read it, and the build fails when the class reference does not list exactly what it holds. Roblox's API dump is generated from the engine's reflection, each member tagged with the contexts that may use it and the security they need, and the engine enforces those tags when a script calls. Factorio publishes its runtime API as a machine-readable file per stage, which its documentation and tools read. The campfire registry follows them:

- **Registration is declaration.** Every name a script may use, a `ctx` call or value, a handle's field or method, is registered by one builder call that both binds the function to Rhai and records the entry: its owner (`ctx`, unit, modifier, projectile, area, damage, position, number), its name, whether it is a value, a call, a field or a method, the roles that may use it, its capability, its signature and a one-line description. No name exists in one place and not the other.
- **One `ctx`.** One `ctx` type serves every role. Its frame holds the role, the acting unit, the params it reads, the mode's state as a mode call sees it, the players' resources and the effects it queues; a call is registered once, for every role design 08 gives it, so a call written for abilities runs in a mode script without a second copy. A call given to some roles only checks the frame's role when it runs, as Roblox checks a member's context, so it fails as the API's refusal even if a check above missed it.
- **One effect queue.** The calls queue effects in one enum; each capability applies its own variants, and they apply in call order, as now.
- **Hooks, states and data fields** are registered by the code that runs them: the system that calls a hook, with its parameters' names, the system that honours a state, the code that reads a data field. A test holds the registry's data fields equal to the names serde reads for each table, and another holds the registry equal to the functions the engine binds, by name and first parameter.
- **Planned names.** A name design 08 gives that no code runs yet is registered as planned, for its roles, with no function. The load check accepts it until the release runs it, and the reference lists it as planned, so what loads and does nothing is visible in one place. At the end of the stage the planned names go, and the load check refuses every name the registry does not run.
- **The checks read the registry.** A `ctx` name loads only for a role it is registered for, of a capability the mode declares. A field or method a script reads on any other value must be one some handle has, one the engine has of its own (Rhai's packages and `Num`'s, which the registry reads from the engine), a key of the script's object-map literals, or one of the script's own functions; the names after `p`, `state` and `params` name data, which the data checks read. A dynamic script gives no type to check against, so the check is by name, and a field read on the wrong handle still fails at run time.
- **The reference is generated.** The tables of `ctx`, handles, hooks, states and data fields come from the registry, into the [reference](08-script-api-reference.md), with each name's roles, capability, status and description; a test fails when the checked-in reference differs from what the registry writes, as Godot's build does for its class reference. The rules stay prose here.

The registry reads Rhai's own list of functions (`Engine::collect_fn_metadata`, from its `internals` feature) for the engine's built-ins, and a test reads it to hold the registry equal to what the engine binds; Rhai holds no roles, capabilities or status, so the registry is campfire's, and Rhai stays its binding.

## Filters

One set of words selects units everywhere: `targeting`, projectile `hits`, area `affects`, aura `affects`, and every query. A filter is a relation, `enemies`, `allies` or `all`, and an optional tag after a colon: `enemies:avatar`, `allies:avatar`, `enemies:creep`. Allies include the unit itself; units of different teams are enemies, so the `neutral` team is an enemy of every other. Avatars, the units players choose, carry the tag `avatar`.

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

**Avatar** (`data/avatar.toml`, in a package of kind `avatar`; the reference MOBA's heroes): `name`, `role`, `resource` (one of the mode's `resources`), `passive` (a modifier id), `slots` (four ability ids; the last is the ultimate), `[combat.attack]` (`range`, `windup_ms`, `projectile_speed`; no speed means melee), `[vision]` (`sight_range`, `true_sight`), `[collision]` (`radius`), `[stats]` (`stat = { base, per_level }`; the value at level `n` is `base + per_level × (n − 1)`), `[abilities.<id>]`, `[modifiers.<id>]`.

**Ability:**

| Field | Meaning |
| --- | --- |
| `script` | Script file, if the ability needs one |
| `targeting` | `none`, `point`, `direction`, or a filter for a unit target |
| `range` | Meters, or `"global"` |
| `cooldown_ms`, `cost`, `cast_time_ms` | Cost is in the avatar's resource |
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

**Modifier:** `script`, `duration_ms` (absent: until removed), `interval_ms`, `stacks_expire_ms`, `reapply` (`refresh` by default, `stack`, `ignore`), `max_stacks` (at least 1; absent: no limit), `stats` (per stack), `states`, `shield` (the modifier ends when the shield is spent), `aura` (`{ radius, affects, modifier }`), `[params]`, `[state]`. A unit holds at most one instance of each modifier id from each source.

**Manifest** (`manifest.toml`): `name`, `version`, `engine` (the release tag it targets) and `kind`: `mode`, `avatar` (a unit players choose, such as a hero) or `loadout` (abilities players pick before spawning, such as spells). A mode's also has `capabilities` (none twice, not `mode`, and each with the ones it builds on: `projectiles`, `abilities` and `vision` on `combat`, `orders` on `combat` and `navigation`), `tick_hz = { min, max, default }` (all positive, `min ≤ default ≤ max`), `teams` (each `{ name, slots }`; their slots in order make the player slots; at most 63, so 64 with the neutral team), `backends`, `max_move_speed` (m/s, positive), `script_limits = { per_call, player, think, mode }` (each pool holds a whole call; each player slot has a pool of the size `player`), and `[dependencies]`, each named by its package's name, and in the workspace given by path. A hero's id is its package's name.

**Mode** (`data/mode.toml`): `script`, `assist_window_ms`, `damage_kinds` and `attack_kind` (the damage kind of every attack, one of `damage_kinds`), both required with the combat capability, `[inputs]` (name and type of each player input: `string`, `string_list`), `[state]`, `[params]` (values, or lists whose entries are values or strings, such as unit types), `[modifiers]`, `levels` (the experience each level needs, from level 2) and `rank_levels` (for `basic` and `ultimate` slots, the level each rank needs) ([Stats](04-capabilities/stats.md#levels-and-experience)). An input that does not match its type never reaches the script. **Units** (`data/units.toml`): `[units.<id>]` with `tags`, `params`, and a section for each capability the unit type uses ([Unit types](04-capabilities/00-overview.md#unit-types)): `stats`, `combat` (`attack = { range, windup_ms, projectile_speed }`, and `on_death`: `stay` or `despawn`, the default), `orders` (`ai`, `think_ms`), `vision` (`sight_range` in meters, not negative, and `true_sight`), `collision` (`radius` of its body in meters, positive, up to 64; a type without it collides with nothing). Health, attack damage, attack speed and move speed are stats: an attack starts at most `attack_speed` times a second, its period the tick rate over that rounded up.

**Map** (`map/map.toml`): `[[paths]]` (`name`, `points` from the first team's end to the second's), `[spawns]` (each playing team's avatar spawn), `[[structures]]` (`unit_type`, `team`, `path` if it guards one, `pos`: they stand from the start), `[[neutral_spawns]]` (`unit_type`, `pos`, as `ctx.map.neutral_spawns` lists them), `[bounds]` (`min` and `max`, `min` below `max` on both axes: no unit is ever outside them), `[grid]` (`cell` in meters, positive: whole cells over the bounds, at most 2²² cells; a mode with `vision` needs it), `[navigation]` (`cell` in meters, the same rules: the cells routes are planned on; a mode with `navigation` needs it). A point is `[x, z]` in meters on the ground plane.

**State types:** `int`, `num`, `bool`, `string`, `entity`, `entity_list`, `pos`, `vec`; each with a `default` and, for mode state, `sync`.

**Stats:** the mode declares every stat its units carry in `data/mode.toml`, each `[stats.<name>]` with `combine` (`sum` or `highest`) and optional `min` and `max`, beside its `damage_kinds` and the `resources` its units spend; the engine reads some by name ([Stats](04-capabilities/stats.md#stats)). A name the mode does not declare fails the package load. The reference MOBA's:

| Stats | Combine | Limits |
| --- | --- | --- |
| `health`, `health_regen`, `resource`, `resource_regen`, `attack_damage`, `ability_power`, `armor`, `magic_resist`, `armor_pen`, `magic_pen`, `move_speed`, `physical_block` | `sum` | — |
| `attack_damage_pct`, `ability_power_pct`, `attack_speed_pct`, `move_speed_pct`, `healing_received_pct`, `damage_dealt_pct` | `sum` | ≥ −1 |
| `attack_speed` | `sum`; attacks a second × (1 + `attack_speed_pct`) | ≤ 2.5 attacks per second |
| `crit_chance`, `armor_pen_pct`, `magic_pen_pct`, `life_steal`, `spell_vamp` | `sum` | 0 to 1 |
| `cooldown_reduction` | `sum` | 0 to 0.4 |
| `slow` | `highest` | 0 to 0.99; `slow_immune` ignores it |

Move speed, attack speed and pools follow [Stats](04-capabilities/stats.md#stats). Every homing projectile flies faster than the mode's `max_move_speed`, which caps move speed, which the package load checks, so it catches its target within launch distance ÷ (projectile speed − cap). `life_steal` heals the source for attack damage dealt and `spell_vamp` for ability damage; `healing_received_pct` scales every heal.

**States:** `stunned`, `rooted`, `silenced`, `disarmed`, `airborne`, `stealthed`, `untargetable`, `slow_immune`.

## Handles

A unit handle has the fields of the capabilities its type uses; reading a field it lacks is a script error. The [reference](08-script-api-reference.md) lists each handle's fields and methods.

## `ctx`

The [reference](08-script-api-reference.md) lists every value and call of `ctx`, with the roles it serves.

**Walking.** A unit a move order, a chase or a path sends somewhere walks a route there on the map's `[navigation]` cells: round structures, then round units that stand in its way or keep it back ([Navigation](04-capabilities/navigation.md#movement)). A goal it cannot stand on ends its route at the nearest place it can.

`ctx.p` reads, in an ability, its params; in a modifier, the modifier's params and then those of the ability that applied it; in a mode or AI script, the mode's params.

**Mode calls.** The map's structures spawn, then `on_match_start` runs, before the first tick. A timer counts from its call, in ticks rounded up, at least one, and fires in the Mode stage at the end of a tick: set before the first tick, 1 ms fires at the end of tick 0. `ctx.teams` names the playing teams, and `ctx.players` counts the session's players; neutral units spawn on `neutral`, and `enemy_team` needs a mode of two playing teams. `spawn_unit` fails for a point outside the map's bounds. `spawn_group` spawns its units in order at the team's end of the path, the first team at its start and the second at its end, walking it. `choose_avatar` takes an avatar no other player chose; `spawn_avatars` spawns each chosen avatar not yet spawned, in slot order, at its team's spawn, its abilities unlearned and its player's loadout at rank 1. `on_unit_died` runs after the due timers, once for each unit that died in the tick, in the order they died; `killer` is `()` when no strike killed it. `respawn` takes a dead unit whose type stays dead, and refuses a living one or one whose type despawns ([Combat](04-capabilities/combat.md#damage-and-death)). A player's mode input is a command of the `mode` owner: its name, then its value in its declared type, in postcard.

## Hooks

The [reference](08-script-api-reference.md) lists every hook, with its parameters, role and capability.

An avatar's passive is a modifier it always carries. `combat` finds takedown participants once, with the mode's `assist_window_ms`: they receive `on_takedown`, and the mode receives them as `assisters`.

## Checks at package load

A package loads only when all of these pass:

- Every data file matches its schema, every per-rank array has one entry for each rank, and every capability field of an ability (`range`, `cooldown_ms`, `cost`, `cast_time_ms`) holds at every rank.
- The mode's unit types declare at most 64 tags together, and no avatar has the name of one of them.
- Every script is referenced by data. Every function named like a hook, a hook's name or any name that starts with `on_`, is a hook of a role the script serves, with the hook's parameters, so a misspelled hook is an error, not a hook that never runs.
- Every modifier id, `ctx.p` name, `{ param }` reference, stat, state, filter and damage kind that a script or data file names exists. Scripts are read with `AST::walk`, from Rhai's `internals` feature.
- Every `ctx` name is one the registry holds, as a value or a call as the script uses it, for a role the script serves, of a capability the mode declares. Every field or method the script reads on another value is one the registry or the engine has, a key of its object maps, or one of its functions ([One source](#one-source)).
- Every value of `ctx` is a variable named `ctx`, so the checks see all its uses: every hook's first parameter is named `ctx`; `ctx` is used only as `ctx.<name>` or as a whole argument of a call, not of an operator or a function pointer's `call` or `curry`; a function of the script that receives it names that parameter `ctx`; and no `let`, `const` or `for` binds a new `ctx`.
- Every capability a package's data or scripts use is declared, and each builds on the ones it needs. A name the registry plans loads until the release runs it, and fails when a script uses it; the reference lists each.
- The manifest's capabilities, tick rates, pools and move speed cap hold, where the manifest is read. Every package targets this release, and every projectile flies faster than the cap.
- The map and the teams name only what the mode has: every structure's and neutral spawn's unit type, team and path; every point of the map within its bounds; every avatar spawn, neutral spawn and waypoint a place the widest unit that walks may stand among the structures, and every waypoint reachable from the one before ([Navigation](04-capabilities/navigation.md#map-checks)); an avatar spawn for each playing team; no team named `neutral`, and no two teams, paths, slots of an avatar or entries of the mode's loadout packages alike.

## Planned changes

- New capabilities add their parts: `items`, `progression`, `interaction`, `production`.

## Open questions

- [ ] Camp monsters respawn one by one; whole camps may be better.
- [ ] Kill streaks and assist rules beyond an even gold split.
- [ ] Items: the same modifier and ability model, not yet written.
