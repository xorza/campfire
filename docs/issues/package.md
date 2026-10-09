# Package

Design: [Modules](../design/02-engine-core.md#modules), `package`. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

- **Stage 8.** Only an avatar names its human text, its `name`; an action, a loadout's spell, a mode, a team and a choice have no message id, so a client can show none of their names, while the design says each name is a message id ([Human text](../design/03-game-scripting.md#game-package)).

## Ready

- **Plan: Z2.** A load walks every file of a package, reads every file under `data/`, `map/`, `scripts/` and `locale/`, and hashes every other file to rebuild the file list, where design 05 reads the package's `package.index` and only the files the session needs, each checked against its row.
- **Plan: Z3.** A package holds one map, `map/map.toml`, where design 03 holds any number under `map/<name>/`.
