# Campfire — Game Scripting

How a game is content: its package, its scripts, its state and its numbers.

## Who runs what

- **Only the server and the verifier run the full sim.** A client would need hidden state; it predicts only what its player controls and runs presentation scripts, and a client that plays back a published log runs the full sim as the verifier does.
- **Game scripts and presentation scripts.** Game scripts (rules, units, abilities, AI) are deterministic and change game state; presentation scripts (effects, sounds, UI) run only on clients and never change it.
- **Bots send inputs.** Bots run outside the sim, see what their team's clients see, and send player inputs.

## Game package

- **One package for each mode.** A mode is one content package with any number of maps; a session's terms name the map it plays ([Keys](05-protocol-spec.md#keys)). Its `package.index` lists every other file's path, size and SHA-256, and is the package's identity ([Packages](05-protocol-spec.md#packages)).
- **Package paths are the same on every OS.** A path is printable ASCII, names no file Windows reserves or refuses, and differs from no other path of its package only in case, as `cargo package` refuses names; an open takes a file only when each name on its way matches its listing's case, as webpack's `CaseSensitivePathsPlugin` checks.
- **Overrides by load order.** A package may override a record of a package it depends on, whole; the last loaded wins, as Bethesda's plugins do, and the load lists every record more than one package overrides.
- **An imported package is an ordinary package.** It has the same layout, checks and fingerprint, depends on its game's rules package, and nobody distributes it ([Zero Hour](12-zero-hour.md)).
- **A versioned package API.** A package targets `api = "major.minor"`, the script API and data schemas together; a release loads its major and a minor no higher than its own, as Factorio loads mods. A binary file's layout is part of the API, so any change raises the major. A log names its exact engine release, so a replay runs the code that recorded it.
- **No human text in data or scripts.** A name or a line is a message id, and `locale/<language>.ftl` gives its text in Mozilla's Fluent, which handles plurals and grammar; text never enters the sim, so a translation changes no result, and a locale package is never a mode's dependency.
- **Assets are untrusted.** Only glTF, PNG, KTX2 and Ogg Vorbis load, each through a pure-Rust decoder, with limits on size, dimensions and counts; no C or C++ decoder reads package data.
- **Trusted UI.** The engine draws prices, payments and wallet prompts; presentation scripts can neither draw nor trigger them.

## Match rules

- **One mode script owns the rules.** The engine knows only waiting, running and ended; phases such as a pick, rounds and overtime are script state.
- **A mode declares its options.** `[options.<name>] = { values, default }` is a param whose value a session's host picks among its numbers, as OpenRA's `PlayerResources` offers `startingcash` of its `SelectableCash` and Zero Hour's `Multiplayer.ini` its starting money; a free value is refused, as a list lets the load check what a session may set, and an option of text is planned. The load refuses an option of no value, of more than 256, of one value twice, of a default none of them, or of a param's name.
- **A script reads the pick as the param.** `ctx.p.<name>` gives the host's pick, else the default, so a script is the same whether a param is an option; the mode installs with the session's picks before `on_match_start`, and a restore target takes them from its terms, so a replay, a verifier and a loaded save play with the session's own.
- **Timers never fire early,** so a mode behaves the same at any tick rate to within one tick.
- **The result is state.** `ctx.end` names the winner, once; the result is sim state, so the final state hash proves it.

## Script state

- **Declared, never invented.** The mode and each unit type declare their fields under `[state]`, each with a type, a default and its `sync` class; a write to an undeclared field, or of another type, fails the call. Unreal declares each replicated property with its condition, as Roblox types its attributes; Dota 2's custom games keep such values where nothing checks them.
- **A component for each sync class.** A unit holds a `UnitState<S>` for each `sync` class its type declares a field of; the values are state, hashed, saved and restored, and each class is a kind of [Replication audiences](14-replication-audiences.md).
- **Writes apply when the call ends.** A write goes to the call's overlay and applies in call order, so a later call of the stage reads it, and a failed call writes nothing ([Scripting](02-engine-core.md#scripting)).
- **Misspellings fail the load.** A literal field name after `.state` must be one some state of the match declares.

## Numbers

- **Two number types, no floats.** Integers (`i64`) for counts, ids and times, and `Num` (`I40F24`) for positions, distances and ratios; arithmetic is checked, and a `Num` becomes an integer only by `floor`, `ceil` or `round`.
- **Decimals as strings in data.** A TOML integer is an integer, a decimal in a string is a `Num`, a float fails the read; a binary format holds a `Num` as its bits.

## Network sync

- **Scripts never deal with networking.** Inputs go to the server as lists of commands, stamped, hash-chained and signed ([Protocol Spec](05-protocol-spec.md#session-log)); each part of the state goes to its audience, script state by its `sync`; clients that see a presentation event run its presentation scripts.
