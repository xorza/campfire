# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Stage 7 — RTS skirmish

The design is [RTS skirmish](docs/design/11-rts-skirmish.md), with the rules in [Space and map](docs/design/04-capabilities/00-overview.md#space-and-map), [Navigation](docs/design/04-capabilities/navigation.md), [Production](docs/design/04-capabilities/production.md) and [Control](docs/design/04-capabilities/control.md#orders). A step that adds a script name writes the [reference](docs/design/08-script-api-reference.md) again from the registry. A step that changes a golden says why in its commit, with the number that moved.

1. **R8. The skirmish** (test packages, runner, CI): the mode of design 11, its map, its faction, and its scripted match with each rule's event asserted in its tick; its golden on every OS, each platform's log replayed on every other. Check: the match plays to its golden in CI on Linux, Windows and macOS; design 11's table and production's and navigation's costs hold measured numbers; the roadmap's Stage 7 loses the summons, production's first cut and the skirmish.
