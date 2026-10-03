# Package

Design: [Modules](../design/02-engine-core.md#modules), `package`. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

- **Stage 10.** Only an avatar names its human text, its `name`; an action, a loadout's spell, a mode, a team and a choice have no message id, so a client can show none of their names, while the design says each name is a message id ([Human text](../design/03-game-scripting.md#game-package)).

## Ready

- **Plan: N1.** Every name the [script API reference](../design/08-script-api-reference.md) lists as planned loads and does nothing: a call or a field fails when a script uses it, and a hook never runs. The 3v3's stun tags block `use`, which no item runs yet.
