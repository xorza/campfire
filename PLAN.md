# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Slice: Skirmish bots

The user watches two Zero Hour bots on Tournament Desert from standard starts, China's attacker and GLA's builder: each builds its base and gathers, and the attacker trains and attacks when its army is ready, until the builder's last building that counts falls; and the user plays a match of their own against such a bot. Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. The play (S)

The user. Last: it ends the slice. Its decisions are in [Game scripting](docs/design/03-game-scripting.md), [Control](docs/design/04-capabilities/control.md), [Sessions](docs/design/08-sessions.md) and [Zero Hour](docs/design/09-zero-hour.md).

- **Check:** the user watches China's attacker beat GLA's builder from standard starts, with `--spectate --server-bot 0=attacker --server-bot 1=builder --pick 0:faction=china --pick 1:faction=gla`, and plays a match against an attacker.

## Slice: Unit variations

A technical, trained or spawned, is one of its chassis, and every rule reads it as one type ([Unit variations](docs/design/unit-variations.md)). Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. Looks by variation (S)

The client, `client-files`. First: the import writes the looks it reads.

- **A look's `variations`,** one look each, chosen by a unit's `Variation`; a test of the client's looks picks each.

### 2. The import of `BuildVariations` (M)

`production`, the import, the Zero Hour check. After 1: it writes the bodies and the looks.

- **One unit type of variations** for a template of `BuildVariations` whose variations differ in body and look alone, a loss written for others; no type of its own for a variation.
- **The train's `variants` go:** a train makes its type, and the spawn draws.
- **Check:** the Zero Hour check's base play: GLA's attacker gives no untaken move, and its technicals go with its army; `golden.toml` blessed once. The slice's decisions fold into production.md, 09-zero-hour.md and the client's document, and its document goes.

## Foundations
