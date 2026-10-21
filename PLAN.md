# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Slice: Base defenses

A player builds a Patriot battery, a tunnel network and a Stinger site and sees each fire at an enemy that comes near, the site's three Stinger soldiers with missiles, each soldier back 30 s after it dies, and the tunnel's two defenders once; and trains Technicals, each of one of three chassis, and Inferno cannons, and sees them fire, and Tomahawks after a Strategy Center ([proposal](docs/design/12-base-defenses.md)). The Scud launcher, which needs a science, the demo trap, which has no body, the weapons of `NOT_AIRBORNE`, and the Tomahawk's weapon, which waits a `PreAttackDelay`, stay dropped. Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. The fold and the play (S)

04-capabilities/production.md, combat.md, 09. Last: it ends the slice.

- **The fold.** The decisions go to production.md, combat.md and 09, with 09's losses: the Stinger soldiers stand by the site's exit, not on its roof, and its damage does not pass to them; the tunnel holds no units; the facts stay in research/zero-hour-base-defenses.md, and the proposal is deleted.
- **Check:** the user plays Tournament Desert in the client and sees a Patriot battery, a tunnel network and a Stinger site fire at the bot's units, a killed Stinger soldier come back, Technicals of each chassis, an Inferno cannon and a Tomahawk fire.

## Foundations
