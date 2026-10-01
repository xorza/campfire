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

Content can come from other packages, referenced by fingerprint. A package may also **override** a record of a package it depends on, a unit type, an action, a modifier, a table or a quest, whole: the mode's load order, its `load_order`, decides, and the last loaded wins, as Bethesda's plugins do. The load lists every record more than one package overrides, so a player sees the conflicts, and a small patch package that loads last settles them.

**Package API.** A package targets a version of the package API, `api = "major.minor"`: the script API and the schemas of the data files together. A release loads every package whose API has its major and a minor no higher than its own, as Factorio loads a mod made for "major.minor" across its patches; a name or field the API adds raises the minor, and one it removes or changes raises the major. The registry records the version each name came in, and the reference lists it. A session log still names its exact engine release, so a replay runs the code that recorded it.

**Human text.** Data and scripts hold no human text: a name, a description, a line of dialogue or a message to the player is a message id, and each package's `locale/<language>.ftl` gives its text, in Mozilla's [Fluent](https://hacks.mozilla.org/2019/04/fluent-1-0-a-localization-system-for-natural-sounding-translations/) format, which handles plurals and grammar for each language, as Space Station 14 keeps all its text in Fluent files. The client shows the player's language, and the package's own, its manifest's `language`, where a message has no translation. A package of kind `locale` adds a language to the packages it depends on, so a translation needs no fork. Text never enters the sim, so a translation changes no result.

**Assets are untrusted.** Clients download them from any server, so only the formats above load, each through a pure-Rust decoder (`gltf`, `png`, `ktx2` + `ruzstd`, `symphonia`); no C or C++ decoder ever reads package data. Loads enforce limits on file size, image dimensions, decompressed size, and vertex and bone counts.

**Trusted UI.** Prices, payments and wallet prompts are drawn by the engine. Presentation scripts can neither draw nor trigger them.

## Tick pipeline

The sim runs each tick in the engine's fixed stages ([Tick stages](04-capabilities/00-overview.md#tick-stages)). A tick sees only its own inputs, in slot order, then seq order. The sim does no I/O: the server logs each input before its tick runs, and sends each client its visible state after it.

## Match rules

One mode script (`scripts/mode.rhai`) owns the rules. The engine knows only waiting, running and ended; everything inside running is the script's.

- **Phases** (hero pick, warmup, rounds, buy time, overtime) are script state, not engine states.
- **Hooks:** `on_match_start` (running begins), `on_player_join`, `on_player_leave`, `on_timer`, `on_mode_input`, plus event hooks from the capabilities in use. A rule that runs each tick or interval over many units is a scripted system ([Scripted systems](04-capabilities/00-overview.md#scripted-systems)). A hook is named `on_<event>` for what happened, or `calc_<value>` for a pure hook that returns a value; every hook takes `ctx` first.
- **Primitives:** timers, freeze and unfreeze, respawn and reset, team changes and relations, players' choices, named per-player resources (e.g. `gold`), scoreboard data.
- **Timers** are set in milliseconds and rounded up to whole ticks (at least one), so a timer never fires early and modes behave the same at any tick rate to within one tick.
- **End:** `ctx.end(team)` names the winning team, and `ctx.end(())` a draw; callable once. The result is sim state, so the final state hash proves it, and from the next stage on no stage runs. Optional: a persistent world never calls it.
- **Saves and carry:** `ctx.save()` asks for a save at the end of the tick; `ctx.carry` reads what the session loaded and writes what it hands on, in the mode's declared `[carry]` schema ([Saves](02-engine-core.md#saves)).

## Capabilities

Units, actions, items, AI hooks and commands come from [capabilities](04-capabilities/00-overview.md). Bots run outside the sim, see what their team's clients see, and send player inputs.

## Script state

Script state is declared, never invented at run time. The mode declares its fields in `data/mode.toml` and each unit type in its own data file, under `[state]`: type (one of the [state types](08-script-api.md#data-files)), default, and replication (`none`, `owner`, `team`, `all`). A `state_version` sits beside them for migrations. A write to an undeclared field, or of the wrong type, is a script error.

```toml
state_version = 1

[state]
phase = { type = "string", default = "warmup", sync = "all" }
round = { type = "int", default = 0, sync = "all" }
```

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
