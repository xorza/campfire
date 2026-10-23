# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Batch: Losses that show

What the sim drops of its inputs is counted where tests and checks see it, and nothing a match gains is lost quietly. The sim, the harness, the players and the import. One commit, ending with `cargo drive gate` passing and a stop for review.

- **The tick counts each command it cannot decode,** by player and addressee, in a report outside the state, as `DroppedOrders` counts its drops; the harness fails a test on any it does not take, as it fails on a warning, and the Zero Hour check pins the count.
- **A player's gain saturates at the most an amount holds,** an income's and a channel's pay among them, and a refund keeps its rule of all or nothing.
- **The supply import names a spawn's failed type as its cause,** not `NoObject`.
- **Check:** a test of an undecodable command's count and of a saturated income; `golden.toml` blessed once.

## Foundations
