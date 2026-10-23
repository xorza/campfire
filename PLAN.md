# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Slice: Skirmish bots

The user watches two Zero Hour bots on Tournament Desert from standard starts, China's attacker and GLA's builder: each builds its base and gathers, and the attacker trains and attacks when its army is ready, until the builder's last building that counts falls; and the user plays a match of their own against such a bot. Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. The play (S)

The user. Last: it ends the slice. Its decisions are in [Game scripting](docs/design/03-game-scripting.md), [Control](docs/design/04-capabilities/control.md), [Sessions](docs/design/08-sessions.md) and [Zero Hour](docs/design/09-zero-hour.md).

- **Check:** the user watches China's attacker beat GLA's builder from standard starts, with `--spectate --server-bot 0=attacker --server-bot 1=builder --pick 0:faction=china --pick 1:faction=gla`, and plays a match against an attacker.

## Slice: Spawn leaders

GLA's angry mob, trained, is a crowd of members that walk, fight and die together, selected by any member ([Spawn leaders](docs/design/spawn-leaders.md)). Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. Led spawns in the sim (L)

`production`, `units`, `orders` and `combat`. First: the batches after it read its leaders.

- **A leader of no life, body or walk,** its place the centre of its living spawns, written in place each tick its spawns move (A unit may lead its spawns).
- **A spawn buds** from the free cells nearest its leader's place, the first ones, and from the living spawn nearest its centre, each later one (A later spawn buds from the nearest spawn).
- **The leader dies with its last spawn,** the last one's killer its killer, its bounty and its death hooks as any unit's (A unit may lead its spawns).
- **An order to a leader is its spawns'**, in increasing stable id, in the one command's units (An order to a leader is its spawns' order).
- **Check:** a leader in a test, as the slice's checks say.

### 2. A member picks as its leader (M)

The client and the replication. After 1: it reads each spawn's leader.

- **Each spawn's leader, state every client receives once;** the pointer's pick of a member selects its leader, and a selected leader rings its members (A member picks as its leader).
- **Check:** a member picks as its leader, as the slice's checks say.

### 3. The import of the mob (M)

The import, the rules package's base lists and the Zero Hour check. After 2: the user plays it in the client.

- **A walker's `SpawnBehavior` of `AggregateHealth`** converts to led spawns, its `ImmortalBody` and harmless weapon to none, its losses written (The import converts a mob).
- **A leader of armed spawns is armed** in the army list (A leader of armed spawns is armed).
- **Check:** the mob in the Zero Hour check, as the slice's checks say; `golden.toml` blessed once. The slice's decisions fold into production.md, control.md, 02-engine-core.md and 09-zero-hour.md, and its document goes.

## Foundations
