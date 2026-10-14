# Campfire — Game Scripting

## Who runs what

Only the server and the verifier run the full sim during a session; clients would need hidden state.

| Program | Runs | Sees |
| --- | --- | --- |
| Server | Full sim and all game scripts | Everything |
| Verifier | Full sim and all game scripts, replaying the log | Everything, after the log is published |
| Client | Prediction of what the player controls, by the capabilities that move it, and presentation scripts | What its audiences give it ([Replication audiences](14-replication-audiences.md)) |
| Spectator client | Presentation scripts, no prediction | What the mode's and the host's `[visibility]` give spectators: every unit through any fog by default, or one team's view or all teams' (D15); behind the host's spectator delay, once it lands |

- **Game scripts** (rules, units, abilities, AI) are deterministic and change game state.
- **Presentation scripts** (effects, sounds, UI) run only on clients and never change game state.
- A client that plays back a published log runs the full sim and all game scripts, like the verifier.

## Game package

A game mode is one content package, which holds any number of maps; a session's terms name the one it plays ([Keys](05-protocol-spec.md#keys)).

```
my-mode/
  manifest.toml       name, version, kind, package API version, capabilities, dependencies,
                      teams and slots, tick-rate range, backends, move speed cap, script pools
                      (the fields: Script API, Data files)
  package.index       every other file's path, size and SHA-256: the package's identity ([Packages](05-protocol-spec.md#packages))
  map/<name>/         one map: bounds, terrain or grid or geometry, paths, placed units, markers
  data/               unit types, actions, modifiers: one section per capability
  scripts/            game scripts (.rhai): mode rules, AI, capability hooks
  client/             presentation scripts (.rhai), and each map's terrain, client/maps/<name>/terrain.bin
  locale/             human text by message id, one Fluent file a language (en.ftl, de.ftl)
  assets/             models (.glb), textures (PNG; KTX2, with or without zstd), sounds (Ogg Vorbis), icons (PNG)
```

**Package paths.** A path names only a file that every OS holds as the same file, as `cargo package` refuses names Windows refuses: printable ASCII alone, as a file system may fold or normalize other letters; no name Windows reserves (`con`, `aux.rhai`); none of `\ : * ? " < > |`; no name that ends in a dot or a space; and no two paths of one package, or directories on their way, that differ only in case. An open takes a file only when each name on its way is in its directory's listing with its row's case, as webpack's `CaseSensitivePathsPlugin` checks, so a name that differs in case is missing on every OS ([Packages](05-protocol-spec.md#packages)). The store finds a package by its `manifest.toml`.

A package's `name` is a lowercase letter, then lowercase letters, digits, hyphens and underscores, as `hero-husk`: a component of a locale file's path, and an avatar's unit type name. Content can come from other packages by fingerprint. A package may **override** a record of a package it depends on, a unit type, an action, a modifier, a table or a quest, whole: in the mode's `load_order` the last loaded wins, as Bethesda's plugins do. The load lists every record more than one package overrides, so a small patch package that loads last settles the conflicts.

**Imported packages.** A package the `import` app writes is an ordinary package, with the same layout, checks and fingerprint; it depends on its game's rules package, and nobody distributes it ([Zero Hour](12-zero-hour.md#decisions), D4).

**Package API.** A package targets `api = "major.minor"`, the script API and the data schemas together. A release loads a package of its major and a minor no higher than its own, as Factorio loads mods across patches; a name or field the API adds raises the minor, one it removes or changes the major. A binary file of a package, postcard with no schema, such as `heights.bin`, is part of the API by its layout: any change raises the major, so a package of another layout fails its load before any file is read. The registry records the version each name came in. The release runs package API 1.0. A log still names its exact engine release, so a replay runs the code that recorded it.

**Human text.** Data and scripts hold no human text: a name, a description, a line or a message is a message id, and `locale/<language>.ftl` gives its text in Mozilla's [Fluent](https://hacks.mozilla.org/2019/04/fluent-1-0-a-localization-system-for-natural-sounding-translations/), which handles plurals and grammar. The client shows each package's own language, its manifest's `language`: the packages are in English until the player's choice of language comes, with every localization (planned). Text never enters the sim, so a translation changes no result.

- **Message ids** are Fluent identifiers: a letter, then letters, digits, `-` and `_`, each package's its own.
- **Names** are what a player picks, buys or casts: `name`, a message id, on the mode, on each team of its manifest, `[teams.<team>] name`, on each choice, each item type, each avatar, and each action of an avatar or a loadout package, which the load requires of each. A mode's other actions, a tower's attack or an item's active, may give one; an item's active shows its item's.
- **Languages** are Unicode language identifiers, `en`, `pt-BR`; a file's name is the canonical spelling, so each language has one file.
- **Load checks.** Every file under `locale/` is `<language>.ftl`, parses, and defines no message twice. Every message id the data names has a value in the own language's file; another language defines only messages the own file defines.
- **Locale packages.** A package of kind `locale` holds `locale/<package>/<language>.ftl` for each package it depends on, under the same checks. Once the player chooses a language (planned), a client finds a message's text in it: the package's own file, then a locale package's, then the package's own language; until then, it loads no locale package. A locale package is never a mode's dependency, so a translation changes no session.

**Assets are untrusted.** Only the formats above load, each through a pure-Rust decoder (`gltf`, `png`, `ktx2` and `ruzstd`, `symphonia`); no C or C++ decoder reads package data. Loads limit file size, image dimensions, decompressed size, and vertex and bone counts.

**Trusted UI.** Prices, payments and wallet prompts are drawn by the engine; presentation scripts can neither draw nor trigger them.

## Tick pipeline

The sim runs each tick in fixed stages ([Tick stages](04-capabilities/00-overview.md#tick-stages)). A tick sees only its own inputs, in slot order, then seq order. The sim does no I/O: the server logs each input before its tick runs, and replicates after it.

## Match rules

One mode script (`scripts/mode.rhai`) owns the rules. The engine knows only waiting, running and ended; everything inside running is the script's.

- **Phases** (hero pick, warmup, rounds, buy time, overtime) are script state, not engine states.
- **Hooks:** `on_match_start`, `on_player_join`, `on_player_leave`, `on_timer`, `on_mode_input`, and the event hooks of the capabilities in use ([Script API](08-script-api.md#rules)). A rule over many units each interval is a scripted system ([Scripted systems](04-capabilities/00-overview.md#scripted-systems)).
- **Primitives:** timers, freeze, respawn and reset, team changes and relations, players' choices, named per-player resources (`gold`), scoreboard data.
- **Timers** never fire early, so modes behave the same at any tick rate to within one tick ([Mode calls](08-script-api.md#ctx)).
- **End:** `ctx.end(team)` names the winner, `ctx.end(())` a draw; callable once. The result is sim state, so the final state hash proves it. From the next stage on no stage or edge runs but the closing set of the stage that ended it ([Determinism rules](02-engine-core.md#bevy)).
- **Saves and carry:** `ctx.save()` asks for a save at the end of the tick; `ctx.carry` reads what the session loaded and writes what it hands on, in the mode's `[carry]` schema ([Saves](02-engine-core.md#saves)).

## Capabilities

Units, actions, items, AI hooks and commands come from [capabilities](04-capabilities/00-overview.md). Bots run outside the sim, see what their team's clients see, and send player inputs.

## Script state

Script state is declared, never invented at run time. The mode declares its fields in `data/mode.toml` and each unit type in its data, under `[state]`: a type ([state types](08-script-api.md#data-files)), a `default`, and `sync`, the clients that receive it. A `state_version` sits beside them for migrations. A write to an undeclared field, or of the wrong type, is a script error.

```toml
state_version = 1

[state]
phase = { type = "string", default = "warmup", sync = "all" }
round = { type = "int", default = 0, sync = "all" }
```

**Unit state.** A unit type's `[state]` declares the fields every unit of it holds; a projectile or an area declares its state in its unit type. Unreal declares each replicated property with a condition of who receives it, and Roblox gives every instance typed attributes; Dota 2's custom games keep such values in Lua tables nothing checks.

- **Where it lives.** A unit holds, for each `sync` class its type declares a field of, a component `UnitState<S>`, its values in the order of the class's field names. The values are state: hashed, saved and restored; a restore refuses a component whose values are not its type's fields of its class, in count or type, and one of a class the type has no field of. The script view reads a unit's classes merged into the order of their names.
- **Reading and writing.** `unit.state.<field>` reads and writes a field of any unit handle, in every role. A write goes to the call's overlay, which the call reads back, and applies when the call ends, in call order, so a later call of the stage reads it; a failed call writes nothing. An undeclared field fails the call with `UnknownState`, a value of another type with `WrongStateType`.
- **New units.** A unit a call creates starts at its type's defaults; the call's writes to it apply as it spawns ([New units at once](08-script-api.md#rules)).
- **Sync.** `owner`: the player who owns the unit; `team`: the owner and its team's players; `all`: every client that sees the unit; `none`, the default: no client. Each class is a kind of [Replication audiences](14-replication-audiences.md). A mode field takes only `none` or `all`, as the match has no owner or team (D7).
- **Checks.** The load checks declared names and defaults of the field's type. A literal name a script reads or writes after `.state` must be a field some state of the match declares, so a misspelling fails the load; a handle's type is unknown to the load, so a field the unit's own type lacks fails only its call.

Effects a call queues apply after it returns: after `ctx.damage(...)`, the target's health changes once the call ends. A failed call changes nothing ([Scripting](02-engine-core.md#scripting)).

## Numbers

Scripts have two number types and no decimal literals (`no_float`):

- **Integers** (Rhai `INT`, `i64`): counts, ids, milliseconds, amounts.
- **`Num`** (`I40F24`): positions, distances, speeds, ratios.

Arithmetic on both is checked; an overflow is a script error. An integer becomes a `Num` automatically, an integer that does not fit being an error; a `Num` becomes an integer only through `floor`, `ceil` or `round` (half away from zero). A `Num` operation with an integer gives a `Num`.

In data files, a TOML integer is an integer and a decimal in a string (`"7.5"`) is a `Num`. Capability fields sit in their capability's section; values for scripts in `[params]`.

## Network sync

Scripts never deal with networking. **Client → server:** inputs, each a list of commands in their capabilities' formats, stamped with a tick, hash-chained and signed per packet with the session key ([Protocol Spec](05-protocol-spec.md#session-log)); the server validates each. **Server → client:** each part of the state to its audience, script state by its `sync` ([Replication audiences](14-replication-audiences.md)). **Presentation events:** the sim emits events (cast, hit, death), and clients that see them run presentation scripts.

## Examples

Full API: [Script API](08-script-api.md). Genre examples are in [Games](04-capabilities/games.md).

**Round-based mode** (`scripts/mode.rhai`; `phase` and `round` are the fields declared above):

```rhai
fn on_match_start(ctx) {
    ctx.state.phase = "warmup";
    ctx.state.round = 0;
    ctx.timer("warmup_end", 60000, false, ());
}

fn on_timer(ctx, name, data) {
    if name == "warmup_end" || name == "round_end" {
        if ctx.state.round == 30 { ctx.end(()); return; }
        ctx.state.round += 1;
        ctx.state.phase = "buy";
        // `frozen` is a modifier of the mode; its tag blocks moving and every action.
        for unit in ctx.avatars() { ctx.add_modifier(unit, "frozen", 15000); }
        ctx.timer("buy_end", 15000, false, ());
    }
    if name == "buy_end" {
        ctx.state.phase = "live";
        ctx.timer("round_end", 115000, false, ());
    }
}
```
