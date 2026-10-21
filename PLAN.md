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

## Foundations
