# Package

Design: [Modules](../design/02-engine-core.md#modules), `package`. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

- A read finds a file by its index path through the OS, so where the file system ignores case, as macOS's and Windows' do by default, a file whose name differs from its row only in case reads and passes its hash, where on Linux it is missing: one package can load on one OS and fail on another.
- **Stage 8.** Only an avatar names its human text, its `name`; an action, a loadout's spell, a mode, a team and a choice have no message id, so a client can show none of their names, while the design says each name is a message id ([Human text](../design/03-game-scripting.md#game-package)).

## Ready

