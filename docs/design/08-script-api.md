# Campfire — Script API

The rules of the script API. Every name, with its roles, capability and status, is in the [reference](08-script-api-reference.md), which the registry writes.

## Rules

- **Derived from what packages need.** A name enters the API only when a package the engine plays needs it: the test packages and the imported games' rules packages.
- **Data first.** What the engine can do from data, it does: effects, deliveries, intervals, modifiers, charges, projectiles, areas, auras, shields; scripts describe only what is special.
- **Hooks by name.** `on_<event>` for what happened, `calc_<value>` for a pure hook that returns a value; hook names differ between roles, so one file serves an action and the modifiers it applies, and an event's data list runs before its hook.
- **Handles read, `ctx` changes.** Handles are read-only but `.state` and a modifier's `.stacks`; every other change goes through `ctx` and is queued, so the call is all or nothing ([Scripting](02-engine-core.md#scripting)).
- **A default source.** `ctx` knows the acting unit, and effects come from it unless a call names another.
- **New units at once.** A call that spawns gets a handle with the new unit's id, to grant actions and write its state; the unit appears when the call commits.
- **Event order.** A call's effects apply in call order; the hooks they trigger queue first in, first out, after them, and a chain past an engine constant fails.
- **A handle keeps a gone unit's last values,** and effects on a dead unit do nothing but a respawn.
- **Queries return living targets in stable-id order.** `find` includes units hidden from the caller, for effects on an area; `find_visible` returns only what its group sees, for choosing a target.
- **Randomness only from the secret streams.** `ctx.chance` and `ctx.pick` draw from one sequence for each acting unit and tick, so draws follow the call order, the same on every machine, and the server alone runs them.
- **Time rounds so nothing happens early.** A wait rounds up to whole ticks, a cut off a wait rounds down, and each kind of time is its own argument type.
- **Vectors and numbers are exact.** Vector sums and scales round once; a place past the world's bound fails the call ([Numbers](03-game-scripting.md#numbers)).

## One source

- **The registry is the one list.** One builder call binds each name to Rhai and records its owner, roles, capability, signature and description, and everything else derives from it, as Godot's `ClassDB` is filled by the call that binds the code, Roblox's API dump comes from reflection, and Factorio publishes its runtime API as a machine-readable file.
- **One `ctx` for every role.** A call is registered once for all its roles, and a call of some roles checks the frame's role as it runs, as Roblox checks a member's context, so it fails even if a load check missed it.
- **Planned names are registered.** A name the design gives that no code runs is registered as planned, and the load refuses it with where it stands.
- **The reference is generated,** and a test fails when the checked-in file differs.

## Filters

- **One set of words selects units everywhere.** A filter is a set of relations, then tags, each optionally negated: `enemies:avatar:!stunned`; it serves targeting, hits, areas, auras, player modifiers, weapons and queries.
- **Deliveries are named to be selected.** A filter selects a projectile, an area or an item only when it names that tag.

## Engine enums

- **The engine's sets are enums, a mode's names strings.** A value of a fixed set the engine owns, as `Relation::Hostile`, reaches a script as an enum member the language checks, as Factorio's `defines`, Roblox's `Enum` and Godot's bound enums do; a mode's names are known only when its packages load, so they stay strings the load checks.
- **A mode's names cannot be constants.** A declared name can be a Rhai keyword, an avatar plays in many modes, and a modifier and a stat live in two scopes a script could not tell apart.

## Data files

- **Arrays in an action are per rank;** anywhere else an array is a list.
- **A scaling param is a stat's formula.** `base + per_level × (level − 1) + Σ ratio × stat` of the source, its sum rounded once, so a scaling param always has a value.
- **Unit types live in their package's scope.** An avatar's summon, a loadout's ward and a faction's units are unit types of their own packages, which only their packages' data and scripts name, so no two packages' types share an identity; a faction as a package of its own waits.

## Checks at package load

- **A misspelling fails the load.** Every name a script or data file gives exists: ids, params, stats, tags, teams, paths, layers, filters, damage kinds and marker tags; a misspelled hook is an error, not a hook that never runs; every script compiles in strict variables mode.
- **A script is its functions.** The load refuses statements outside a script's functions, which no call would run, and the engine refuses `try` and `catch`, so no script swallows a refusal of the API that the failure list exists to show; `throw` stays, as it fails its call as a refusal does. Roblox's `pcall` takes the other choice.
- **`ctx` stays visible.** Every value of `ctx` is a variable named `ctx`, and no script makes a function pointer, as a closure that captured `ctx` would hide it from the checks.
- **Every time counts at the fastest rate.** Each time and think period counts in ticks at the fastest rate the manifest allows, so it counts at every rate a session may choose, and a match start fails only on its terms.
- **Every modifier application gives its params,** at every rank it applies at.
- **Every unit type spawns sound.** Each type that stands makes a unit at level 1 whose pools' maxima are positive.
