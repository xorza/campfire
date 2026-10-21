# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Slice: Base defenses

A player builds a Patriot battery, a tunnel network and a Stinger site and sees each fire at an enemy that comes near, the site's three Stinger soldiers with missiles, each soldier back 30 s after it dies, and the tunnel's two defenders once; and trains Technicals, each of one of three chassis, and Inferno cannons, and sees them fire, and Tomahawks after a Strategy Center. The Scud launcher, which needs a science, the demo trap, which has no body, the weapons of `NOT_AIRBORNE`, and the Tomahawk's weapon, which waits a `PreAttackDelay`, stay dropped. Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. The play (S)

The user. Last: it ends the slice. Its decisions are in [Production](docs/design/04-capabilities/production.md) and [Zero Hour](docs/design/09-zero-hour.md).

- **Check:** the user plays Tournament Desert in the client and sees a Patriot battery, a tunnel network and a Stinger site fire at the bot's units, a killed Stinger soldier come back, Technicals of each chassis and an Inferno cannon fire, and a Tomahawk train after a Strategy Center.

## Slice: Test matches

The user starts the client as a spectator of Tournament Desert, where two bots, China and America, each start with a base of every building its faction builds and two of each unit it trains, and send their armies at each other; and the user starts a match of their own on such a base. Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. The play (S)

The user. Last: it ends the slice. Its decisions are in [Sessions](docs/design/08-sessions.md), [Navigation](docs/design/04-capabilities/navigation.md), [Production](docs/design/04-capabilities/production.md) and [Zero Hour](docs/design/09-zero-hour.md).

- **Check:** the user watches Tournament Desert's two bots fight from their bases in the client, with `source/bots/zero-hour/tournament_desert/start1-china-base-attacker.toml` and `start2-america-base-attacker.toml`, and plays a match of their own on a base, with `--pick 0:start=base`.

## Slice: Skirmish bots

The user watches two Zero Hour bots, China and America, on Tournament Desert from standard starts: each builds its base, trains, and attacks when its army is ready, until one side's last building that counts falls; and the user plays a match of their own against such a bot ([proposal](docs/design/12-skirmish-bots.md)). Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. A bot of Rhai (L)

`script`, `bots`, 06-script-api-reference.md. First: every batch after it runs bots of Rhai.

- **The role.** `ScriptRole` (`capabilities/core/script/src/scripts/script_role.rs:8`) gains `Bot`; a bot host compiles the bots a mode's data lists, each a script and its params, under the manifest's limits, and the load checks a bot script's names against the bot API as it checks the others'. Builds: a name is resolved once.
- **The bot API.** A bot's call reads, through `BotView` (`session/bots/src/bot_view.rs:23`), its seat's player, resources and choices, its own living units by type and their sites, the units its team sees, the map's markers and the mode's params; and gives directives by the order scripts' names, turned into payloads through `BotSink`, so the API and the order scripts share one map of each directive. Builds: what a module adds is declared once.
- **Check:** tests of a test mode's bot script that reads each part of its view and orders a move, a train and a build, each payload as the order script of the same orders gives it; a name the bot API lacks refused at the load; the reference written again.

### 2. The driver and the command line (M)

`net`, `server`, `client`, 08-sessions.md. After batch 1, whose bots it drives.

- **The driver.** `BotDriver` (`network/net/src/sim_server/bot_driver.rs:35`) drives a seat by an order script or by a mode's bot, which it calls every `think_ms`, the seats staggered, within a bot budget; a restore builds each bot again and plays it on from the world it finds. Builds: what can be forgotten is a type.
- **The command line.** `--server-bot <seat>=<bot>` names a mode's bot, and `--server-orders <seat>=<file>` an order script, on the server (`apps/server/src/command_line/mod.rs:46`) and the client (`apps/client/src/command_line/mod.rs:88`); a name the mode lacks is refused before a session opens. Builds: types over strings.
- **Check:** net tests of a session whose bot of Rhai thinks at its period and whose inputs the log holds as `Bot` inputs, of a restore that plays it on, and of a bot whose think finds its budget spent and waits; the arguments of each flag, and a bot name the mode lacks refused.

### 3. A building's place and what a seat may do (M)

`production`, `bots`. After batch 1, independent of batch 2.

- **The place.** The bot API's `place_near(builder, build, point, gap, steps)` walks the grid round `point` as `room_near` does, to the first point where `Placement::passes` (`capabilities/production/src/placement.rs:51`) holds for the build of `builder`'s team, or none. Builds: one home for each operation.
- **What a seat may do.** The bot API tells whether a seat's unit may start a train or a build now: its slot holds the action, its requirements hold, its player affords it and its queue has room, as the order's own checks give it. Builds: one home for each operation.
- **Check:** tests of the place by hand-computed points, past a wall, a body and a seen enemy, and none past its steps; of each reason a train or a build may not start.

### 4. Zero Hour's builder and attacker (L)

The Zero Hour rules package, `import`, the Zero Hour check, 09-zero-hour.md. After batches 2 and 3, which it uses.

- **The bots.** The rules package ships `scripts/bot.rhai`, and the import's mode data names it `builder` and `attacker`, with `think_ms` and the attacker's `army` and `guard`: the builder builds each building of `base_buildings_<faction>` it lacks, with its builder, at `place_near` of its start, and does nothing once its base is whole; the attacker does too, trains its army from its producers, and sends it at the nearest enemy start once it holds `army` units, attacking the nearest enemy it sees there; while it gathers, its army attacks the nearest enemy it sees within `guard` of its start.
- **The samples and the check.** `source/bots/` goes, and AGENTS.md's layout with it; the skirmish play's builder becomes an order script of the check, a fixture; the check plays two attackers from base starts until one side's last building that counts falls, and a builder against an attacker from standard starts, and pins both ends.
- **Check:** `cargo drive run zero-hour`, on request, pins both matches' ends, each with a result within its limit.

### 5. The fold and the play (S)

03-game-scripting.md, 04-capabilities/control.md, 08-sessions.md, 09-zero-hour.md. Last: it ends the slice.

- **The fold.** The decisions go to game scripting, control, which drops its planned decision of a skirmish bot of rules, sessions and Zero Hour; the facts stay in research/test-matches.md, and the proposal is deleted.
- **Check:** the user watches two attackers fight from standard starts until one falls, and plays a match against an attacker.

## Foundations
