# Core

Design: [Capabilities](../design/04-capabilities/00-overview.md): the core under every capability. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

- The match end stops each stage, `MatchEnd::running` on every `SimSet`, but not the gaps between them: the passes in `SimEdge::Start` and each `SimEdge::After`, `stats`' refreshes and `navigation`'s `keep_in_bounds`, run on after the match ended. The options: give the gaps the same condition, or keep them running and state why.

## Research

- The script view copies the whole match for each script batch: `ScriptBatch::run` reads every unit again and fills its row from all seven column sources, though few units change between two batches of one tick. `View::read` takes 14.2 % of a 3v3 match's time, in Think and the damage pass most ([Hot stages](../design/13-benches.md#hot-stages), Resolve).

## Ready

