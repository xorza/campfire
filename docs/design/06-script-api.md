# Campfire — Script API

The rules of the script API. Every name, with its roles, capability and status, is in the [reference](06-script-api-reference.md), which the registry writes.

## Rules

- **Derived from what packages need.** A name enters the API only when a package the engine plays needs it: the test packages and the imported games' rules packages.
- **Data first.** What the engine can do from data, it does: effects, deliveries, intervals, modifiers, charges, projectiles, areas, auras, shields; scripts describe only what is special.
- **One vocabulary.** A change of `ctx` is a verb, `<verb>_<object>`, and a query a noun or `find_*`; a predicate is `is_`, `has_` or `can_`; each write has its read and each add its remove; arguments go subject, which, how much, how long, then an optional source; and one word names one concept, `action` and `unit` among them, as the Rust API Guidelines ask of a crate's names and Godot's API names its predicates. Refused: the names each capability's first author chose, which gave `end` beside `on_end`, `charge` beside stored charges, and `target` two meanings.
- **Hooks by name.** `on_<event>` for what happened, `calc_<value>` for a pure hook that returns a value; hook names differ between roles, so one file serves an action and the modifiers it applies, and an event's data list runs before its hook.
- **Handles read, `ctx` changes.** Handles are read-only but `.state` and a modifier's `.stacks`; every other change goes through `ctx` and is queued, so the call is all or nothing ([Scripting](02-engine-core.md#scripting)).
- **A default source.** `ctx` knows the acting unit, and effects come from it unless a call names another.
- **An AI reads and orders.** An AI's call reads the match, draws from its stream and orders the unit that thinks, as a player gives its orders; every call that changes the match fails in it, and an order names no unit, so an AI gets no more than a player, as Dota 2's bot scripts and BWAPI's clients only issue orders. Refused: an AI that queues effects, which a player could not do, and orders for another unit, which a player's commands give only to units they own.
- **New units at once.** A call that spawns gets the new unit's `Unit` handle, to grant actions, write its state and keep it in state or a timer's data; the unit appears when the call commits, and a read of its other values fails the call until then, as they do not exist yet. Refused: a second handle type for a new unit, which a script could not store.
- **Event order.** A call's effects apply in call order; the hooks they trigger queue first in, first out, after them, and a chain past an engine constant fails.
- **A handle keeps a gone unit's last values,** and effects on a dead unit do nothing but a respawn.
- **Queries return living targets in stable-id order.** `find` includes units hidden from the caller, for effects on an area; `find_visible` returns only what its group sees, for choosing a target.
- **What a team sees is one rule of the view.** Each view column answers whether it hides a unit's row from a team, and only the vision column hides any, so `find_visible`, `nearest_visible` and `can_see` read one rule, and a match with no vision shows every team every unit, as a game with no fog does; the queries are the core's, so a mode with no vision, as Zero Hour's, gives them too. Refused: the queries in the vision module alone, which a mode with no vision lacks, so its bots could not ask what they see; a bot's `find` in such a mode, which a mode with vision would turn into a read of what its team does not see.
- **A list gives the living.** Every query that lists units gives the living alone, `ctx.avatars()`, `ctx.units_tagged(tag)` and `player.units` among them, and a name that says `all` gives the dead too, `ctx.all_avatars()`, so a script that counts a dead unit says so, as Dota 2's `FindUnitsInRadius` leaves the dead out unless its flag asks for them. Refused: lists that gave the dead for some names and not others, which a script learned by reading each description.
- **Randomness only from the secret streams.** `ctx.chance` and `ctx.pick` draw from one sequence for each acting unit and tick, so draws follow the call order, the same on every machine, and the server alone runs them.
- **Time rounds so nothing happens early.** A wait rounds up to whole ticks, a cut off a wait rounds down, and each kind of time is its own argument type.
- **Vectors and numbers are exact.** Vector sums and scales round once; a place past the world's bound fails the call ([Numbers](03-game-scripting.md#numbers)).

## One source

- **The registry is the one list.** One builder call binds each name to Rhai and records its holder, roles, capability, signature and description, and everything else derives from it, as Godot's `ClassDB` is filled by the call that binds the code, Roblox's API dump comes from reflection, and Factorio publishes its runtime API as a machine-readable file.
- **A holder is its registered type.** A member's holder is the type the API registers for scripts to hold, a `ScriptHandle` of the module that registers it, with its name in scripts and its title in the reference, or `ctx`, as Godot's `ClassDB` keys each method by its class. Refused: an enum of every handle in the core, which named each capability's types.
- **A hook is its caller's type.** Each hook is a type of the module whose system calls it, with its name, its role, its parameters and the types of its call's arguments, and the module declares it with its status, so the registry, the load and a match's book of scripts read every hook from the declarations, and a call passes the arguments its hook's type takes, as Bevy's events are the types of the plugins that send them. Refused: one enum of every hook in the core, which named each capability's events and let a call pass any tuple of the right length.
- **A queue of mode events answers itself.** A queue of events the mode's scripts answer, as level-ups, deaths and seat events, implements one trait: the hooks that answer it, how its tick's events arrive, how each is answered and what an event that waits holds, as a death keeps its unit; one loop of the mode answers every queue, the events that wait first, up to the first call that waits, as Bevy's `Events<E>` is one queue for every event type. The mode installs the loop for each queue it answers. Refused: a hand-written loop for each queue, which repeated the waiting rule; and queues that install their own loops, which would order systems of other modules' sets in the mode's stage.
- **One `ctx` for every role.** A call is registered once for all its roles, and a call of some roles checks the frame's role as it runs, as Roblox checks a member's context, so it fails even if a load check missed it.
- **Planned names are registered.** A name the design gives that no code runs is registered as planned, and the load refuses it with where it stands.
- **Data fields come from their types.** A data type's fields are what serde reads of it, and a table's keys are the sections its modules declare, so no list beside the schema can drift, as Godot's `ClassDB` lists properties from the code that binds them; a module that owns some fields of a type several modules read names them, and each must be a field of the type.
- **A table is its reader's.** A data table is a title that the module which reads it gives its type, or, for the mode's file and a unit type, whose keys every module's sections are, the core, so the core lists no capability's table. Refused: an enum of every table in the core.
- **The reference is generated,** and a test fails when the checked-in file differs.

## Filters

- **One set of words selects units everywhere.** A filter is a set of relations, then tags, each optionally negated: `enemies:avatar:!stunned`; it serves targeting, hits, areas, auras, player modifiers, weapons and queries.
- **Deliveries are named to be selected.** A filter selects a projectile, an area or an item only when it names that tag.

## Engine enums

- **The engine's sets are enums, a mode's names strings.** A value of a fixed set the engine owns, as `Relation::Hostile`, reaches a script as an enum member the language checks, as Factorio's `defines`, Roblox's `Enum` and Godot's bound enums do; a mode's names are known only when its packages load, so they stay strings the load checks.
- **A mode's names cannot be constants.** A declared name can be a Rhai keyword, an avatar plays in many modes, and a modifier and a stat live in two scopes a script could not tell apart.
- **Rhai keeps its module system.** The engine enums are static modules, `Relation::Hostile`, which Rhai's `no_module` removes; the host's raw engine, with no module resolver and room for no module, already refuses an `import`.

## Data files

- **Arrays in an action are per rank;** anywhere else an array is a list.
- **A scaling param is a stat's formula.** `base + per_level × (level − 1) + Σ ratio × stat` of the source, its sum rounded once, so a scaling param always has a value.
- **Unit types live in their package's scope.** An avatar's summon, a loadout's ward and a faction's units are unit types of their own packages, which only their packages' data and scripts name, so no two packages' types share an identity; a faction as a package of its own waits.

## Checks at package load

- **A misspelling fails the load.** Every name a script or data file gives exists: ids, params, stats, tags, teams, paths, layers, filters, damage kinds and marker tags; a misspelled hook is an error, not a hook that never runs; every script compiles in strict variables mode.
- **A name kind is its type's.** The kind of a name that scripts and data give is the `Named` kind of the type whose values the names are, with its text, its article and whether its scope is a package's, so the type that resolves a name gives its kind, beside the ids in `values` and in each capability's crate; a kind the load does not know fails it loudly. Refused: an enum of every kind in the script crate, which named each capability's names.
- **A returned name is checked where a script compares it.** A form that gives a name says its kind by the type of its value, and the load checks each literal a script compares with it by `==`, `!=` or `in`, as TypeScript refuses a comparison of a literal union with a literal outside it; it refuses a `switch` on such a name, whose cases Rhai keeps only as hashes. The name stays text as it runs, so it joins a list, keys a map and goes back as an argument; a typed handle would need an operator for each of these, and would check the literal only as the call runs.
- **A scoped name compares through a method.** A name of a package's scope, as an action's, returns as its text alone, which another package's action may share, so the load refuses its comparison with a literal; the handle's method, as `d.is_action(name)`, reads the literal in the script's package, as every argument of that kind does.
- **A name runs since a version.** Each record of the registry holds one `Status`, the package API version it runs since or planned, and the load refuses a script's name that runs since a version past the one its package targets, as Android's lint refuses a call above the app's `minSdkVersion`; so a package reads the same on every release that loads it.
- **A script is its functions.** The load refuses statements outside a script's functions, which no call would run, and the engine refuses `try` and `catch`, so no script swallows a refusal of the API that the failure list exists to show; `throw` stays, as it fails its call as a refusal does. Roblox's `pcall` takes the other choice.
- **`ctx` stays visible.** Every value of `ctx` is a variable named `ctx`, and Rhai builds with no closures, so no function captures it and an anonymous function or a pointer is one more function of the script; a pointer's call takes `ctx` only as an argument the checks refuse. Refused: a ban on every function pointer, which refused lambdas that capture nothing.
- **Every time counts at the fastest rate.** Each time and think period counts in ticks at the fastest rate the manifest allows, so it counts at every rate a session may choose, and a match start fails only on its terms.
- **Every modifier application gives its params,** at every rank it applies at.
- **Every unit type spawns sound.** Each type that stands makes a unit at level 1 whose pools' maxima are positive.
- **A name's kind reaches lists, hook arguments and variables,** and the load refuses only what it proves wrong. Refused: typed handles, which A returned name is checked where a script compares it refuses.
