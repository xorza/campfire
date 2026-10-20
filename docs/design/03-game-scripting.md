# Campfire — Game Scripting

How a game is content: its package, its scripts, its state and its numbers.

## Who runs what

- **Only the server and the verifier run the full sim.** A client would need hidden state; it predicts only what its player controls, and a client that plays back a published log runs the full sim as the verifier does.
- **Game scripts, and presentation scripts planned.** Game scripts (rules, units, abilities, AI) are deterministic and change game state. Presentation scripts (effects, sounds, UI) are planned: they will run only on clients and never change it, and no script role, host or API holds them yet.
- **Bots send inputs.** Bots run outside the sim, see what their team's clients see, and send player inputs.

## Game package

- **One package for each mode.** A mode is one content package with any number of maps; a session's terms name the map it plays ([Keys](05-protocol-spec.md#keys)). Its `package.index` lists every other file's path, size and SHA-256, and is the package's identity ([Packages](05-protocol-spec.md#packages)).
- **Package paths are the same on every OS.** A path is printable ASCII, names no file Windows reserves or refuses, and differs from no other path of its package only in case, as `cargo package` refuses names; an open takes a file only when each name on its way matches its listing's case, as webpack's `CaseSensitivePathsPlugin` checks.
- **Overrides by load order.** A package may override a record of a package it depends on, whole; the last loaded wins, as Bethesda's plugins do, and the load lists every record more than one package overrides. A rules package's `data/rules.toml` is mode data under the mode's own, each rules package over the ones its mode names before it: each key of a section is a record, as `[combat] armors`, and so is each named entry of a table of them, as a state field, so a mode gives one field and keeps the rest. Refused: a whole section as one record, which made an import write the fixed fields of a section again beside the one it gives.
- **A rules package states what it needs.** Its manifest names the capabilities its scripts and data use and the tick rates they run at, and the load refuses a mode that lacks a capability or allows a rate outside them, as a Factorio mod's `info.json` names what it needs and the game refuses a set of mods that lacks it. Refused: a check as a script first runs, which a match of a rare rule reaches late.
- **Defaults under each unit type.** A mode's `[defaults.<section>]` lies under the section of each of its unit types that gives that section, its avatars' and its packages' content among them, before the type reads it, so the type's own field stays and a type that gives no such section takes none; a rules package gives defaults as it gives any mode data, by load order. Zero Hour's `ThingFactory::newTemplate` copies its `DefaultThingTemplate` into each new template, as StarCraft II's catalogs give an entry a `default` parent. The load refuses a default of no unit type's section, or of a field its section does not read. Refused: a declaration in a capability's `declare` for each field a mode may default, which needs code and a key name for each; and no general mechanism, which leaves each default its own code.
- **An imported package is an ordinary package.** It has the same layout, checks and fingerprint, depends on its game's rules package, and nobody distributes it ([Zero Hour](09-zero-hour.md)).
- **A versioned package API.** A package targets `api = "major.minor"`, the script API and data schemas together; a release loads its major and a minor no higher than its own, as Factorio loads mods. A binary file's layout is part of the API, so any change raises the major. A log names its exact engine release, so a replay runs the code that recorded it.
- **No human text in data or scripts.** A name or a line is a message id, and `locale/<language>.ftl` gives its text in Mozilla's Fluent, which handles plurals and grammar; text never enters the sim, so a translation changes no result, and a locale package is never a mode's dependency.
- **Assets are untrusted.** Only glTF, PNG, KTX2 and Ogg Vorbis load, each through a pure-Rust decoder, with limits on size, dimensions and counts; no C or C++ decoder reads package data.
- **Trusted UI.** The engine draws prices, payments and wallet prompts; a presentation script, planned, can neither draw nor trigger them.

## Match rules

- **The engine knows only waiting, running and ended.** Phases such as a pick, rounds and overtime are script state.
- **Rules scripts in order.** A mode's `data/mode.toml` lists its scripts under `scripts`, its own and its rules packages', and an event's hook runs in each script that defines it, in the list's order, as Factorio calls every mod's handlers in load order. The entry marked `mode = true`, or the list's only one, is the mode's own: it alone holds the mode's state and the players' choices and ends the match, so a rules package's script may be it, as the skirmish's is Zero Hour's. A rules script declares no state until a rules package gives it data, the load refuses its use of what only the own script may, and a timer belongs to the script that sets it. Refused: the mode package's own script alone as the one that ends the match, which moves a game's victory out of its rules package; and each script's state declared in `mode.toml` by its package's name, which makes the mode know its rules packages' internals.
- **An event waits in the scripts that did not answer it.** An event whose call in a later script finds the mode's pool spent waits with how many of the scripts answered it, and a later tick runs it in the rest, so no call runs twice. Refused: one event's calls as one call, all or none, which needs the pool to hold the whole event.
- **A pure hook in one script.** The load refuses a second script of a mode's list that defines a pure hook, as `calc_damage` or `calc_heal`, so no order the list hides weighs a value. Refused for now: a chain in the list's order, as BAR's gadget handler passes `UnitPreDamaged`'s damage on by layer, whose source warns that the order matters.
- **A mode declares its options.** `[options.<name>] = { values, default }` is a param whose value a session's host picks among its numbers, as OpenRA's `PlayerResources` offers `startingcash` of its `SelectableCash` and Zero Hour's `Multiplayer.ini` its starting money; a free value is refused, as a list lets the load check what a session may set, and an option of text is planned. The load refuses an option of no value, of more than 256, of one value twice, of a default none of them, or of a param's name.
- **A script reads the pick as the param.** `ctx.params.<name>` gives the host's pick, else the default, so a script is the same whether a param is an option; the mode installs with the session's picks before `on_match_start`, and a restore target takes them from its terms, so a replay, a verifier and a loaded save play with the session's own.
- **Timers never fire early,** so a mode behaves the same at any tick rate to within one tick.
- **The result is state.** `ctx.win` names the winner and `ctx.draw` none, once, and `ctx.result` reads it; the result is sim state, so the final state hash proves it.

## Script state

- **Declared, never invented.** The mode and each unit type declare their fields under `[state]`, and the mode each player's under `[player_state]`, each with a type, a default and its `audience` class; a write to an undeclared field, or of another type, fails the call. Unreal declares each replicated property with its condition, as Roblox types its attributes; Dota 2's custom games keep such values where nothing checks them.
- **A component for each audience class.** A unit holds a `UnitState<S>` for each `audience` class its type declares a field of, and a player's entity a `PlayerState<S>` for each class, its owner the player; the values are state, hashed, saved and restored, and each class is a kind of [Replication audiences](10-replication-audiences.md).
- **An enum field holds one of its members.** A field of type `enum` declares its `values`, and holds the text of one, so a phase reads as `ctx.state.phase == "play"` and a write of any other text fails the call, as Unreal's `UENUM` properties and Godot's enum exports hold one of a declared list. Refused: a plain string, which takes a misspelled phase in silence; and a member by number, which a script reads as nothing.
- **Writes apply when the call ends.** A write goes to the call's overlay and applies in call order, so a later call of the stage reads it, and a failed call writes nothing ([Scripting](02-engine-core.md#scripting)).
- **Misspellings fail the load.** A literal field name after `.state` must be one some state of the match declares.

## Numbers

- **Two number types, no floats.** Integers (`i64`) for counts, ids and times, and `Num` (`I40F24`) for positions, distances and ratios; arithmetic is checked, and a `Num` becomes an integer only by `floor`, `ceil` or `round`.
- **Decimals as strings in data.** A TOML integer is an integer, a decimal in a string is a `Num`, a float fails the read; a binary format holds a `Num` as its bits.

## Network sync

- **Scripts never deal with networking.** Inputs go to the server as lists of commands, stamped, hash-chained and signed ([Protocol Spec](05-protocol-spec.md#session-log)); each part of the state goes to its audience, script state by its `audience`; clients that see a presentation event will run its presentation scripts, which are planned.
