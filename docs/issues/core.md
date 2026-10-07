# Core

Design: [Capabilities](../design/04-capabilities/00-overview.md): the core under every capability. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

- The script view copies the whole match for each script batch: `ScriptBatch::run` reads every unit again and fills its row from all seven column sources, though few units change between two batches of one tick. `View::read` takes 14.2 % of a 3v3 match's time, in Think and the damage pass most ([Hot stages](../design/13-benches.md#hot-stages), Resolve).

## Ready

- **Plan: M2.** `Grid::spans_within` takes each row's root with `u128::isqrt`, 24.6 % of `fog/sight` and 2.8 % of `server_tick/mean_3v3`, where `math`'s exact root takes less than half the time ([Integer roots](../design/14-integer-roots.md)).

