# Campfire — Game Scripting

## Who runs what

Only the server and verifier run the full sim; clients would need hidden state.

| Program | Runs | Sees |
| --- | --- | --- |
| Server | Full sim + all game scripts | Everything |
| Verifier | Full sim + all game scripts, replaying the session log | Everything, after the match |
| Client | Prediction of what the player controls, plus presentation scripts | Only what its team can see |

- **Game scripts** (rules, units, abilities, AI) are deterministic and change game state.
- **Presentation scripts** (effects, sounds, UI) run only on clients and can never change game state.

## Game package

A game mode is one content package.

```
my-mode/
  manifest.toml       id, version, engine release, kits used, dependencies (by fingerprint),
                      teams and slots, tick-rate range, collision, pathfinding and visibility backends
  map/                map data: geometry or grid, spawn points, structures
  data/               kit data (MOBA: units, abilities; FPS: weapons)
  scripts/            game scripts (.rhai): mode rules, AI, kit hooks
  client/             presentation scripts (.rhai)
  assets/             models, textures, sounds, icons
```

Content can come from other packages, referenced by fingerprint.

## Tick pipeline

Runs every tick on the server and the verifier, in this fixed order:

1. **Apply inputs** (core): player, bot and external inputs recorded for this tick.
2. **Kit systems** (kit + script): movement, combat, AI; kits call their script hooks.
3. **Collision** (core): backend chosen by the mode.
4. **Mode hooks** (script): timers and kit events; `match.end` ends the match.
5. **Visibility** (core): backend marks what each client may see.
6. **Record and send** (core): log inputs; send each client its visible state and events.

## Match rules

One mode script (`scripts/mode.rhai`) owns the rules. The engine knows only waiting, running and ended; everything inside running is the script's.

- **Phases** (hero pick, warmup, rounds, buy time, overtime) are script data, not engine states.
- **Hooks:** `on_match_start` (running begins), `on_player_join`, `on_player_leave`, `on_timer`, plus event hooks from the kits in use. `on_tick` exists but is discouraged.
- **Primitives:** timers, freeze and unfreeze, respawn and reset, team changes, named per-player resources (e.g. `gold`), scoreboard data.
- **Timers** are set in milliseconds and converted to ticks, so modes behave the same at any tick rate.
- **End:** `match.end(result)`, callable once. Optional: a persistent world never calls it.

## Kits

Units, abilities, weapons, AI hooks and input formats come from kits; see Game Kits. Bots replace players: they run outside the sim and send player inputs.

## Network sync

Scripts never deal with networking.

**Client → server: inputs** in the kit's format (MOBA: orders; FPS: per-tick input frames), stamped with a tick and signed with the session key. The server validates each input.

**Server → client:** only what each client may see:

| Data | Sent to |
| --- | --- |
| Kit components | Per kit defaults: everyone who sees the entity, or owner or team only |
| Script data | Per field: `none` (default), `owner`, `team` or `all` |
| Entities the visibility backend hides | Nobody on the other team |

**Presentation events.** The sim emits events (cast, hit, death); clients that can see them run presentation scripts for effects and sound.

**Client timeline**

- Other entities are interpolated: shown slightly in the past, smoothly.
- What the player controls is predicted with the same sim code; the kit decides what else is predicted.
- Wrong predictions are corrected by rollback (Lightyear).

## Examples

Function names show the shape of the API. Scripts have no decimal literals (`no_float`): decimal values come from data files, time is in milliseconds.

**Round-based mode, genre-neutral** (`scripts/mode.rhai`; `phase` and `round` are script data sent to `all`). Kit examples are in Game Kits.

```rhai
fn on_match_start(m) {
    m.data.phase = "warmup";
    m.data.round = 0;
    m.timer("warmup_end", 60000, false);
}

fn on_timer(m, name) {
    if name == "warmup_end" || name == "round_end" {
        if m.data.round == 30 { m.end(m.leading_team()); return; }
        m.data.round += 1;
        m.data.phase = "buy";
        m.respawn_all();
        m.freeze_all(true);
        m.timer("buy_end", 15000, false);
    }
    if name == "buy_end" {
        m.data.phase = "live";
        m.freeze_all(false);
        m.timer("round_end", 115000, false);
    }
}
```
