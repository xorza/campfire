# Campfire — Script API

Derived from the reference packages in `source/packages/moba/`, six heroes, the player spells and the 3v3 mode, and from what the genre proofs need ([Genres](04-capabilities/genres.md#genre-proofs)). Every name is used there; a name neither needs is not here. The [reference](08-script-api-reference.md), which the registry writes, lists every name with its roles, capability and whether the release runs it.

Each call, handle field and hook belongs to the core or to one capability ([Capabilities](04-capabilities/00-overview.md)); a package gets those of the capabilities it declares, and the load checks refuse the rest. The reference names each one's capability.

## Rules

- **Data first.** What the engine can do from data, it does: effects of actions, deliveries and intervals, held modifiers, charges, charged actions, projectiles, areas, auras, shields. Scripts describe only what is special about an effect.
- **Hooks by name.** The engine calls a script function by its hook name: `on_<event>` for what happened, `calc_<value>` for a pure hook that returns a value. Hook names differ between roles (action, modifier, mode, AI), so one file can serve an action and the modifiers it applies. An event's data list and its hook share the name, and the list runs first.
- **Handles read, `ctx` changes.** Handles are read-only, except `.state` and a modifier's `.stacks`, which a call may write and read back. Every other change goes through `ctx` and is queued: the call is all-or-nothing ([Scripting](02-engine-core.md#scripting)).
- **Default source.** `ctx` knows the acting unit: the unit whose action runs, the source of a projectile or an area, or a modifier's source. Damage, heals, modifiers, projectiles and areas come from it unless a call names another.
- **New units at once.** A handle to a unit created in a call is usable in that call, including `.state`; the unit appears when the call commits.
- **Event order.** A call's effects apply in call order. The hooks they trigger are queued first-in first-out and run after all of the call's effects. A hook of a modifier that is gone by then is skipped. A chain longer than an engine constant fails with `script_error`.
- **Dead and despawned units.** A handle keeps returning the unit's last values, and `alive` is false. Effects on a dead unit do nothing, except `respawn`. A modifier whose source despawned keeps that source's last stats.
- **Values are live.** `ctx.p` resolves when it is read, with the current rank and the current stats of the source.
- **Pure hooks.** `calc_damage` and `calc_heal` return a value; their `ctx` refuses effects.
- **Handles last one call.** A call sees the units as its stage began; a handle is valid within the call. `unit.target` is `()` when the unit has no target, or its target is gone. `unit.recent_attackers(ms)` lists the living units that struck it within the last `ms`, rounded up to whole ticks.
- **Queries.** Lists are sorted by stable id; `nearest_visible` sorts by distance, then id. `find`, `find_visible` and `nearest_visible` never return dead units or units whose tags block `target`; `avatars` and `units_tagged` return the dead too, so a mode sees a structure that fell. `find` includes units hidden from the caller's group, for effects on an area; `find_visible` and `nearest_visible` return only what that group sees, for choosing a target, and `unit.can_see(other)` tells whether the unit's group sees `other`. What a group sees is what the Vision stage of the last tick found; without `vision`, every group sees every unit. Distances are in the map's metric.
- **Numbers.** [Game Scripting](03-game-scripting.md#numbers). `num(i)` makes a `Num` from an integer. `Num` has `min`, `max`, `clamp`, `round`, `floor`, `ceil`; `round` and the others return integers. The operators, the comparisons, `min` and `max` take an integer on either side.
- **Randomness** comes only from `ctx.chance` and `ctx.pick`, both from the secret stream. An attack's roll is drawn by `combat`.
- **Time** is in milliseconds, rounded up to whole ticks.
- **Rhai names.** Rhai's reserved words cannot be names, including `spawn` and `match`.

## One source

The registry replaced the hand-kept lists the load check read (a table of `ctx` names beside three `ctx` types that each registered their own), which drifted from what the engine runs: a name loaded that no code ran, or that one role ran and another did not.

Established engines keep one source for what scripts may use, and derive everything else from it. Godot's `ClassDB` is filled by the same `bind_method` call that binds the code; the script analyzer and the editor read it, and the build fails when the class reference does not list exactly what it holds. Roblox's API dump is generated from the engine's reflection, each member tagged with the contexts that may use it and the security they need, and the engine enforces those tags when a script calls. Factorio publishes its runtime API as a machine-readable file per stage, which its documentation and tools read. The campfire registry follows them:

- **Registration is declaration.** Every name a script may use, a `ctx` call or value, a handle's field or method, is registered by one builder call that both binds the function to Rhai and records the entry: its owner (`ctx`, unit, modifier, damage, heal, position, number), its name, whether it is a value, a call, a field or a method, the roles that may use it, its capability, its signature and a one-line description. No name exists in one place and not the other.
- **One `ctx`.** One `ctx` type serves every role. Its frame holds the role, the acting unit, the params it reads, the mode's state as a mode call sees it, the players' resources and the effects it queues; a call is registered once, for every role design 08 gives it, so a call written for abilities runs in a mode script without a second copy. A call given to some roles only checks the frame's role when it runs, as Roblox checks a member's context, so it fails as the API's refusal even if a check above missed it.
- **One effect queue.** The calls queue effects in one enum; each capability applies its own variants, and they apply in call order, as now.
- **Hooks, tag effects and data fields** are registered by the code that runs them: the system that calls a hook, with its parameters' names, the system that asks a tag effect, the code that reads a data field. A test holds the registry's data fields equal to the names serde reads for each table, and another holds the registry equal to the functions the engine binds, by name and first parameter.
- **Planned names.** A name design 08 gives that no code runs yet is registered as planned, for its roles, with no function. The load check accepts it until the release runs it, and the reference lists it as planned, so what loads and does nothing is visible in one place. At the end of the stage the planned names go, and the load check refuses every name the registry does not run.
- **The checks read the registry.** A `ctx` name loads only for a role it is registered for, of a capability the mode declares. A field or method a script reads on any other value must be one some handle has, one the engine has of its own (Rhai's packages and `Num`'s, which the registry reads from the engine), a key of the script's object-map literals, or one of the script's own functions; the names after `p`, `state` and `params` name data, which the data checks read. A dynamic script gives no type to check against, so the check is by name, and a field read on the wrong handle still fails at run time.
- **The reference is generated.** The tables of `ctx`, handles, hooks, tag effects and data fields come from the registry, into the [reference](08-script-api-reference.md), with each name's roles, capability, status and description; a test fails when the checked-in reference differs from what the registry writes, as Godot's build does for its class reference. The rules stay prose here.

The registry reads Rhai's own list of functions (`Engine::collect_fn_metadata`, from its `internals` feature) for the engine's built-ins, and a test reads it to hold the registry equal to what the engine binds; Rhai holds no roles, capabilities or status, so the registry is campfire's, and Rhai stays its binding.

## Filters

One set of words selects units everywhere: `targeting`, projectile `hits`, area `affects`, aura `affects`, player modifiers, weapons, and every query. A filter is a relation, then tags, each after a colon and each with an optional `!` that negates it: `enemies`, `allies:avatar`, `enemies:avatar:!stunned`, `enemies:air`. The relations are those of the teams ([Relations](04-capabilities/00-overview.md#relations)): `enemies` hostile and neutral, the units that may be attacked; `hostiles` hostile; `neutrals` neutral; `allies` friendly, the unit itself included; `all` every one. The tags are a unit's whole set ([Tags](04-capabilities/stats.md#tags)): its type's, those the engine gives it, and those its modifiers grant. A relation selects units with the `projectile`, `area` or `item` tag only when the filter names that tag.

## Data files

Arrays in the fields and params of an action are per rank: one entry for each rank of its slot kind. Anywhere else an array is a list.

**Param values** (`[params]`, read as `ctx.p.<name>`; an unknown name is an error):

| Form | Example | Value |
| --- | --- | --- |
| Integer | `slow_ms = 2000` | the integer |
| Decimal string | `radius = "3.0"` | a `Num` |
| Array | `slow = ["0.15", "0.20", "0.25", "0.30", "0.35"]` | the entry for the current rank |
| Scaling table | `{ base = [80, 120, 160, 200, 240], ability_power = "0.65" }` | `base + per_level × level + Σ ratio × stat` of the source |

A scaling table's keys are `base`, `per_level`, and a ratio for any stat the mode declares, by its name; under `bonus`, a ratio of the part of a stat above its type's value: `{ base = 60, bonus = { attack_damage = "0.5" } }`. A number field anywhere in data may be `{ param = "<name>" }`, resolved in the same way. An action's `range`, `cooldown_ms`, `cost` and `windup_ms` resolve when the action loads, one value for each rank, so a param they name is a value or a per-rank array, not a scaling table.

**Manifest** (`manifest.toml`): `name`, `version`, `engine` (the release tag it targets) and `kind`: `mode`, `avatar` (a unit type players choose, such as a hero) or `loadout` (actions players choose, such as spells). A mode's also has `capabilities` (none twice, not `mode`, and each with the ones it builds on: `projectiles`, `abilities` and `vision` on `combat`, `orders` on `combat` and `navigation`), `tick_hz = { min, max, default }` (all positive, `min ≤ default ≤ max`), `teams` (each `{ name, slots }`, any number; their slots in order make the player slots; a team with no slots, such as a MOBA's camps, has only units), `backends`, `max_move_speed` (m/s, positive), `script_limits = { per_call, player, think, mode, systems }` (each pool holds a whole call; each player slot has a pool of the size `player`), and `[dependencies]`, each named by its package's name, and in the workspace given by path. An avatar's id is its package's name.

**Mode** (`data/mode.toml`): `script`, `assist_window_ms`, `damage_kinds` (with the combat capability, at least one), `[combat]` ([Combat](04-capabilities/combat.md#data)), `[stats]`, `[pools]` and `[tags]` ([Stats](04-capabilities/stats.md#data)), `[slots]` ([Actions](04-capabilities/actions.md#data)), `layers` ([Navigation](04-capabilities/navigation.md#data)), `[[relations]]` ([Relations](04-capabilities/00-overview.md#relations)), `[choices]` ([Choices](04-capabilities/control.md#choices)), `levels` ([Progression](04-capabilities/progression.md#data)), `resources` (the players' resources), `[inputs]` (name and type of each player input: `string`, `string_list`), `[state]`, `[params]` (values, or lists whose entries are values or strings, such as unit types), `[modifiers]`, `[abilities]` and `[systems]` ([Scripted systems](04-capabilities/00-overview.md#scripted-systems)). An input that does not match its type never reaches the script.

**Units** (`data/units.toml` of a mode, or the one unit type of an avatar package): `[units.<id>]` with `tags`, `params`, `[state]`, and a section for each capability the unit type uses ([Unit types](04-capabilities/00-overview.md#unit-types)): `stats` (`stat = { base, per_level }`), `pools`, `abilities` (action ids by slot kind), `passive` (a modifier it holds from its spawn), `combat` (`on_death`), `orders`, `character`, `vision`, `collision` (`radius`, `layer`), `hitscan`, `item`, `inventory`, `use`, `enter`, `node`, `drop_off`, `projectile`, `area`. An avatar package's unit type has `name` too, and its `[abilities.<id>]` and `[modifiers.<id>]`.

**Actions** (`[abilities.<id>]`): [Actions](04-capabilities/actions.md#data). **Modifiers** (`[modifiers.<id>]`): [Stats](04-capabilities/stats.md#data).

**Map** (`map/map.toml`): `metric` (`planar` or `spatial`), `[bounds]` (`min` and `max`, `min` below `max` on every axis: no unit is ever outside them), `[grid]` (`cell` in meters, positive: whole cells over the bounds, at most 2²² cells; a mode with grid vision needs it), `[navigation]` (`cell`, the same rules, and the cells each layer blocks; a mode with `navigation` needs it), `[[paths]]` (`name`, `points`), `[[units]]` (`unit_type`, `team`, `pos`, and `path` with `from` if it walks or guards one: they stand from the start), `[[markers]]` (`name`, `tags`, `pos` or a region, `team` and `params`, each optional but `name` and `tags`). A point is `[x, z]` in meters on the ground plane, or `[x, y, z]` with `spatial`.

**State types:** `int`, `num`, `bool`, `string`, `entity`, `entity_list`, `pos`, `vec`; each with a `default` and, for mode state, `sync`.

**The reference MOBA's stats, pools and tags** (in `data/mode.toml`; the engine reads some by name or binding ([Stats](04-capabilities/stats.md#stats))):

| Stats | Limits |
| --- | --- |
| `health`, `health_regen`, `mana`, `mana_regen`, `energy`, `energy_regen`, `attack_damage`, `ability_power`, `armor`, `magic_resist`, `armor_pen`, `magic_pen`, `move_speed`, `physical_block` | — |
| `healing_received_pct`, `damage_dealt_pct` | ≥ −1 |
| `attack_speed` | ≤ 2.5 attacks per second |
| `crit_chance`, `armor_pen_pct`, `magic_pen_pct`, `life_steal`, `spell_vamp` | 0 to 1 |
| `cooldown_reduction` | 0 to 0.4 |

Pools: `health`, `mana` and `energy`, each with its stat and its regen. `[combat]`: `life = "health"`, `leech = { attack = "life_steal", other = "spell_vamp" }`, `heal_scale = "healing_received_pct"`; its `calc_damage` crits when `d.roll` is below the source's `crit_chance`. A slow is a `cut` of `move_speed`, and every slow modifier grants the tag `slowed`. Slots: `attack`, `basic` (5 ranks, at levels 1, 3, 5, 7, 9), `ultimate` (3 ranks, at 6, 11, 16) and `spell` (the loadout).

| Tag | Effects |
| --- | --- |
| `stunned`, `airborne` | `blocks = ["move", "attack", "cast", "use"]` |
| `rooted` | `blocks = ["move"]` |
| `silenced` | `blocks = ["cast"]` |
| `disarmed` | `blocks = ["attack"]` |
| `untargetable` | `blocks = ["target"]` |
| `invulnerable` | `blocks = ["target", "damage"]` |
| `stealthed` | `hidden = true` |
| `true_sight` | `detects = true` |
| `slow_immune` | `immune = ["slowed"]` |

## Handles

A unit handle has the fields of the sections its type has; reading a field it lacks is a script error. Projectiles, areas and items are units, with the fields of their sections. The [reference](08-script-api-reference.md) lists each handle's fields and methods.

## `ctx`

The [reference](08-script-api-reference.md) lists every value and call of `ctx`, with the roles it serves.

**Walking.** A unit a move order, a chase or a path sends somewhere walks a route there on its layer's `[navigation]` cells: round structures, then round units that stand in its way or keep it back ([Navigation](04-capabilities/navigation.md#movement)). A goal it cannot stand on ends its route at the nearest place it can.

`ctx.p` reads, in an action, its params; in a modifier, the modifier's params and then those of the action that applied it; in a mode or AI script, the mode's params.

**Mode calls.** The map's placed units spawn, then `on_match_start` runs, before the first tick. A timer counts from its call, in ticks rounded up, at least one, and fires in the Mode stage at the end of a tick: set before the first tick, 1 ms fires at the end of tick 0. `ctx.teams` names the teams, and `ctx.players` counts the session's players; `enemy_team` needs a mode of two teams with slots. `ctx.map.markers(tag)` lists the markers of a tag, each with its `name`, `pos`, `team` and `params`. `spawn_unit(type, team, pos)` and `(type, team, pos, player)` fail for a point outside the map's bounds. `spawn_group(team, path, from, types)` spawns its units in order at the path's `from` end, `"start"` or `"end"`, walking it from there. `choose`, `chosen` and `available` read and record players' choices ([Choices](04-capabilities/control.md#choices)); `grant` puts actions in a unit's slot kind. `on_unit_died` runs after the due timers, once for each unit that died in the tick, in the order they died; `killer` is `()` when no damage of a source killed it. `respawn` takes a dead unit whose type stays dead, and refuses a living one or one whose type despawns ([Combat](04-capabilities/combat.md#death-and-respawn)). `set_relation(a, b, relation)` changes how two teams regard each other. A player's mode input is a command of the `mode` owner: its name, then its value in its declared type, in postcard.

## Hooks

The [reference](08-script-api-reference.md) lists every hook, with its parameters, role and capability.

A unit type's `passive` is a modifier it always carries. `combat` finds takedown participants once, with the mode's `assist_window_ms`: they receive `on_takedown`, and the mode receives them as `assisters`.

## Checks at package load

A package loads only when all of these pass:

- Every data file matches its schema, every per-rank array has one entry for each rank of its slot kind, and every field of an action (`range`, `cooldown_ms`, `cost`, `windup_ms`) holds at every rank. A field an action's kind does not use fails.
- The mode declares at most 256 tags together, and no avatar has the name of one of them.
- Every script is referenced by data. Every function named like a hook, a hook's name or any name that starts with `on_` or `calc_`, is a hook of a role the script serves, with the hook's parameters, so a misspelled hook is an error, not a hook that never runs.
- Every modifier id, action id, `ctx.p` name, `{ param }` reference, stat, pool, tag, slot kind, layer, filter, damage kind, choice and marker tag that a script or data file names exists. Scripts are read with `AST::walk`, from Rhai's `internals` feature.
- Every `ctx` name is one the registry holds, as a value or a call as the script uses it, for a role the script serves, of a capability the mode declares. Every field or method the script reads on another value is one the registry or the engine has, a key of its object maps, or one of its functions ([One source](#one-source)).
- Every value of `ctx` is a variable named `ctx`, so the checks see all its uses: every hook's first parameter is named `ctx`; `ctx` is used only as `ctx.<name>` or as a whole argument of a call, not of an operator or a function pointer's `call` or `curry`; a function of the script that receives it names that parameter `ctx`; and no `let`, `const` or `for` binds a new `ctx`.
- Every capability a package's data or scripts use is declared, and each builds on the ones it needs. A name the registry plans loads until the release runs it, and fails when a script uses it; the reference lists each.
- The manifest's capabilities, tick rates, pools and move speed cap hold, where the manifest is read. Every package targets this release, and every homing projectile flies faster than the cap.
- The map and the teams name only what the mode has: every placed unit's unit type, team and path; every marker's tags and team; every point of the map within its bounds; every spawn marker and waypoint a place the widest unit of its layer that walks may stand among the placed units, and every waypoint reachable from the one before ([Navigation](04-capabilities/navigation.md#map-checks)); no two teams, paths, markers of one name or entries of a choice alike; every relation naming two teams of the mode.

