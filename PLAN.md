# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Stage 5: MOBA mechanics

Every mechanic design 07 lists, run from the reference packages, in the order each needs the one before: the core's draws and vectors, the actions over time, reveals, forced movement, terrain and brush, items, the 3v3 scenario that casts them all, and last the load that refuses a planned name, once no reference package uses one. Each step runs a name or field the reference lists as planned, and marks it as running.

21. **S1. The 3v3 casts** (runner): the scripted 3v3's players learn and cast each hero's abilities and use the spells, and buy, use and sell items, beside stage 4's scenario; the match test checks each mechanic by its computed numbers. The 3v3 golden is blessed.
22. **N1. Planned names refused** (package, [One source](docs/design/08-script-api.md#one-source)): the load refuses a planned name in a script, a planned data field and a planned hook, with the name and where it stands. Tests: one flaw each; every reference package loads.
