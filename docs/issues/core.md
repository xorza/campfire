# Core

Design: [Capabilities](../design/04-capabilities/00-overview.md): the core under every capability. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

- The script view copies the whole match for each script batch: `ScriptBatch::run` reads every unit again and fills its row from all seven column sources, though few units change between two batches of one tick. `View::read` takes 14.2 % of a 3v3 match's time, in Think and the damage pass most.

## Ready

- **Plan: S2.** `Grid::spans` takes a sight's rows one by one, each with a scalar root and two `i64` divisions by the cell, though the rows are independent and fit 64 bits: their arithmetic takes about 60 % of `fog/sight` ([SIMD](../design/11-simd.md)).

