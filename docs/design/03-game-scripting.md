# Campfire — Game Scripting

## Who runs what

Only the server and verifier run the full sim during a session; clients would need hidden state.

| Program | Runs | Sees |
| --- | --- | --- |
| Server | Full sim + all game scripts | Everything |
| Verifier | Full sim + all game scripts, replaying the session log | Everything, after the log is published |
| Client | Prediction of what the player controls, by the capabilities that move it, plus presentation scripts | Only what its vision group can see |
| Spectator client | Presentation scripts, no prediction | Everything, delayed by the host's spectator delay |

- **Game scripts** (rules, units, abilities, AI) are deterministic and change game state.
- **Presentation scripts** (effects, sounds, UI) run only on clients and can never change game state.
- A client that plays back a published log runs the full sim and all game scripts, like the verifier.

## Game package

A game mode is one content package.

```
my-mode/
  manifest.toml       id, version, kind (mode, avatar, loadout, campaign, locale), package API version,
                      capabilities used,
                      dependencies (by fingerprint),
                      teams and slots, tick-rate range, collision, pathfinding and visibility backends,
                      move speed cap, script pools
  map/                map data: bounds, terrain or grid or geometry, paths, placed units, markers
  data/               unit types, actions, modifiers: one section per capability
  scripts/            game scripts (.rhai): mode rules, AI, capability hooks
  client/             presentation scripts (.rhai)
  locale/             human text by message id, one Fluent file a language (en.ftl, de.ftl)
  assets/             models (.glb), textures (PNG, KTX2 + zstd), sounds (Ogg Vorbis), icons (PNG)
```

A package's name, its manifest's `name`, is a lowercase letter, then lowercase letters, digits, hyphens and underscores, as `hero-husk`: one component of a locale file's path, and an avatar's unit type name. Content can come from other packages, referenced by fingerprint. A package may also **override** a record of a package it depends on, a unit type, an action, a modifier, a table or a quest, whole: the mode's load order, its `load_order`, decides, and the last loaded wins, as Bethesda's plugins do. The load lists every record more than one package overrides, so a player sees the conflicts, and a small patch package that loads last settles them.

**Package API.** A package targets a version of the package API, `api = "major.minor"`: the script API and the schemas of the data files together. A release loads every package whose API has its major and a minor no higher than its own, as Factorio loads a mod made for "major.minor" across its patches; a name or field the API adds raises the minor, and one it removes or changes raises the major. The registry records the version each name came in, and the reference lists it. The release runs package API 1.0, which every name and field it has came in; a package of another major, or of a newer minor, fails its load with the version it names. A session log still names its exact engine release, so a replay runs the code that recorded it.

**Human text.** Data and scripts hold no human text: a name, a description, a line of dialogue or a message to the player is a message id, and each package's `locale/<language>.ftl` gives its text, in Mozilla's [Fluent](https://hacks.mozilla.org/2019/04/fluent-1-0-a-localization-system-for-natural-sounding-translations/) format, which handles plurals and grammar for each language, as Space Station 14 keeps all its text in Fluent files. The client shows the player's language, and the package's own, its manifest's `language`, where a message has no translation. A package of kind `locale` adds a language to the packages it depends on, so a translation needs no fork. Text never enters the sim, so a translation changes no result.

- **Message ids.** A message id is a Fluent identifier: a letter, then letters, digits, `-` and `_`. Each package has its own ids: its data names messages of its own `locale/` files, so two packages may both name `hero-name`.
- **Languages.** `language` is a Unicode language identifier, such as `en` or `pt-BR`, as [Fluent](https://projectfluent.org/) and the browsers name languages. A file's name is the identifier in its canonical spelling, `locale/pt-BR.ftl`, so each language has one file.
- **Load checks.** Every file under `locale/` is `<language>.ftl` and parses with no error, and defines no message twice. Every message id the package's data names has a value in the file of its own language. Another language's file defines only messages that the own file defines, so a typo in a translation fails the load instead of never showing.
- **Locale packages.** A locale package holds `locale/<package>/<language>.ftl` for each package it depends on, by that package's name, under the same checks against that package's own file. A client gathers the locale packages it holds and finds the text of a message in the player's language: the package's own file of that language first, then a locale package's, then the package's own language.
- **Text and sessions.** A package's `locale/` files are among its files, so its fingerprint covers its own text, and a session names the text its packages shipped with. A locale package is never a dependency of a mode, so a translation changes no session, and the sim reads no `locale/` file, so a match's hashes are the same in every language.

**Assets are untrusted.** Clients download them from any server, so only the formats above load, each through a pure-Rust decoder (`gltf`, `png`, `ktx2` + `ruzstd`, `symphonia`); no C or C++ decoder ever reads package data. Loads enforce limits on file size, image dimensions, decompressed size, and vertex and bone counts.

**Trusted UI.** Prices, payments and wallet prompts are drawn by the engine. Presentation scripts can neither draw nor trigger them.

## Tick pipeline

The sim runs each tick in the engine's fixed stages ([Tick stages](04-capabilities/00-overview.md#tick-stages)). A tick sees only its own inputs, in slot order, then seq order. The sim does no I/O: the server logs each input before its tick runs, and sends each client its visible state after it.

## Match rules

One mode script (`scripts/mode.rhai`) owns the rules. The engine knows only waiting, running and ended; everything inside running is the script's.

- **Phases** (hero pick, warmup, rounds, buy time, overtime) are script state, not engine states.
- **Hooks:** `on_match_start` (running begins), `on_player_join`, `on_player_leave`, `on_timer`, `on_mode_input`, plus event hooks from the capabilities in use; how hooks are named and called is [Script API](08-script-api.md#rules)'s. A rule that runs each tick or interval over many units is a scripted system ([Scripted systems](04-capabilities/00-overview.md#scripted-systems)).
- **Primitives:** timers, freeze and unfreeze, respawn and reset, team changes and relations, players' choices, named per-player resources (e.g. `gold`), scoreboard data.
- **Timers** never fire early, so modes behave the same at any tick rate to within one tick ([Mode calls](08-script-api.md#ctx)).
- **End:** `ctx.end(team)` names the winning team, and `ctx.end(())` a draw; callable once. The result is sim state, so the final state hash proves it, and from the next stage on no stage runs, nor any pass between two stages, `SimEdge::Start` and each `SimEdge::After`, but the closing set of the stage that ended it, which belongs to that stage ([Determinism rules](02-engine-core.md#bevy)). Optional: a persistent world never calls it.
- **Saves and carry:** `ctx.save()` asks for a save at the end of the tick; `ctx.carry` reads what the session loaded and writes what it hands on, in the mode's declared `[carry]` schema ([Saves](02-engine-core.md#saves)).

## Capabilities

Units, actions, items, AI hooks and commands come from [capabilities](04-capabilities/00-overview.md). Bots run outside the sim, see what their team's clients see, and send player inputs.

## Script state

Script state is declared, never invented at run time. The mode declares its fields in `data/mode.toml` and each unit type in its own data file, under `[state]`: type (one of the [state types](08-script-api.md#data-files)), `default`, and `sync`, the clients that receive it (`none`, `owner`, `team` or `all`). A `state_version` sits beside them for migrations. A write to an undeclared field, or of the wrong type, is a script error.

```toml
state_version = 1

[state]
phase = { type = "string", default = "warmup", sync = "all" }
round = { type = "int", default = 0, sync = "all" }
```

**Unit state.** A unit type's `[state]` declares the fields every unit of it holds, in the same form as the mode's; a projectile or an area is a unit, so a delivery type declares its state the same way, in its unit type, not in its `projectile` or `area` section. Established engines give an object's script values a declared type in the same way: Unreal declares each replicated property of a class with a condition that says who receives it (`COND_OwnerOnly`, `COND_SkipOwner`), and Roblox gives every instance typed attributes; Dota 2's custom games keep such values in Lua tables that nothing checks, which is what declared fields replace.

- **Where it lives.** Each unit holds its values in the order of its type's field names, a component of the core's units; a type with no fields gives its units none. The values are state: hashed, saved and restored, and a restore refuses a unit whose values are not its type's fields, in count or in type.
- **Reading and writing.** `unit.state.<field>` reads and writes a field of any unit handle, in every role. A write goes to the call's overlay, which the call reads back, and applies to the unit when the call ends, in call order, so a later call of the same stage reads it; a failed call writes nothing. A field the unit's type does not declare fails the call with `UnknownState`, and a value not of the field's type with `WrongStateType`, as on `ctx.state`.
- **New units.** A unit a call creates starts at its type's defaults; the call's writes to it apply as it spawns ([New units at once](08-script-api.md#rules)).
- **Sync.** A field's `sync` says which clients receive it: `owner` the player who owns the unit, `team` its team's players, `all` every client that sees the unit, and `none`, the default, no client. The book of the unit types' fields keeps each field's class, for `net` to send each to its clients.
- **Checks.** The load checks a unit type's fields as the mode's: declared names, and a `default` of the field's type. A literal name a script reads or writes after `.state` must be a field some state of the match declares, the mode's, a modifier's or a unit type's, so a misspelled field fails the load; a script's handle has no type the load knows, and may be any unit of the match, so a field the unit's own type lacks fails only its call.

Effects a call queues apply after it returns: after `ctx.damage(...)`, the target's health changes only once the call ends. A failed call changes nothing; see [Scripting](02-engine-core.md#scripting).

## Numbers

Scripts have two number types and no decimal literals (`no_float`):

- **Integers** (Rhai `INT`, `i64`): counts, ids, milliseconds, amounts.
- **`Num`** (`I40F24`): positions, distances, speeds, ratios.

Arithmetic on both is checked; an overflow is a script error. An integer becomes a `Num` automatically, and an integer that does not fit is an error. A `Num` becomes an integer only through `floor`, `ceil` or `round` (half away from zero). A `Num` operation with an integer gives a `Num`.

In data files, a TOML integer is an integer and a decimal in a string (`"7.5"`) is a `Num`. Capability fields sit in their capability's section; values for scripts sit in a `[params]` table.

## Network sync

Scripts never deal with networking.

**Client → server: inputs**, each a list of commands in their capabilities' formats (orders, per-tick input frames, mode inputs), stamped with a tick, hash-chained and signed per packet with the session key; see [Protocol Spec](05-protocol-spec.md#session-log). The server validates each input.

**Server → client:** only what each client may see:

| Data | Sent to |
| --- | --- |
| Capability components | Each capability's defaults: everyone who sees the entity, or owner or team only |
| Script state | Per field, as its schema declares: `none` (default), `owner`, `team` or `all` |
| Units the visibility backend hides from a vision group | Nobody in that group |

**Presentation events.** The sim emits events (cast, hit, death); clients that can see them run presentation scripts for effects and sound.

## Examples

Full API: [Script API](08-script-api.md).

**Round-based mode, genre-neutral** (`scripts/mode.rhai`; `phase` and `round` are the state fields declared above). Genre examples are in [Genres](04-capabilities/genres.md).

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
