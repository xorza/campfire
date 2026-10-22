# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Slice: Skirmish bots

The user watches two Zero Hour bots on Tournament Desert from standard starts, China's attacker and GLA's builder: each builds its base and gathers, and the attacker trains and attacks when its army is ready, until the builder's last building that counts falls; and the user plays a match of their own against such a bot. Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. The play (S)

The user. Last: it ends the slice. Its decisions are in [Game scripting](docs/design/03-game-scripting.md), [Control](docs/design/04-capabilities/control.md), [Sessions](docs/design/08-sessions.md) and [Zero Hour](docs/design/09-zero-hour.md).

- **Check:** the user watches China's attacker beat GLA's builder from standard starts, with `--spectate --server-bot 0=attacker --server-bot 1=builder --pick 0:faction=china --pick 1:faction=gla`, and plays a match against an attacker.

## Foundations
