# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Slice: Spawn leaders

GLA's angry mob, trained, is a crowd of members that walk, fight and die together, selected by any member ([Spawn leaders](docs/design/spawn-leaders.md)). Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. The import of the mob (M)

The import, the rules package's base lists and the Zero Hour check. Last: the user plays it in the client.

- **A walker's `SpawnBehavior` of `AggregateHealth`** converts to led spawns, its `ImmortalBody` and harmless weapon to none, its losses written (The import converts a mob).
- **A leader of armed spawns is armed** in the army list (A leader of armed spawns is armed).
- **Check:** the mob in the Zero Hour check, as the slice's checks say; `golden.toml` blessed once. The slice's decisions fold into production.md, control.md, 02-engine-core.md and 09-zero-hour.md, and its document goes.

## Foundations
