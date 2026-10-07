# Math

Design: [Modules](../design/02-engine-core.md#modules), `math`. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

## Ready

- **Plan: M1.** `math` keeps its exact root to itself: `Num::from_root_of_bits` is `pub(crate)`, takes values below 2¹²⁶ only, and runs its integer steps on `u128` also for a value below 2⁶⁴, where they save only a sixth against `u128::isqrt` ([Integer roots](../design/14-integer-roots.md)).
