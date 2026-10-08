# Package

Design: [Modules](../design/02-engine-core.md#modules), `package`. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

- **Stage 10.** Only an avatar names its human text, its `name`; an action, a loadout's spell, a mode, a team and a choice have no message id, so a client can show none of their names, while the design says each name is a message id ([Human text](../design/03-game-scripting.md#game-package)).

## Ready

- **Plan: F12.** A package path may name a file that some OS cannot hold, so one tree is a package on one OS and none on another: `PackagePath::parse` refuses only an empty name, `.`, `..` and a backslash, and accepts a name Windows reserves, as `con.rhai` or `aux`, a character it refuses, as `:`, `*`, `?`, `"`, `<`, `>` or `|`, a name that ends in a dot or a space, and two paths of one package that differ only in case, which a case-insensitive file system, Windows' and macOS's by default, holds as one file. `PackageStore` finds a package by `manifest.toml` with `is_file`, which on such a file system also finds `Manifest.toml`, and on Linux does not.
- **Plan: F13.** `PackageDir::read_disk` walks each directory in the order `fs::read_dir` gives, which differs by OS and file system, and returns its first flaw, so for a tree with two flaws, a link and a name no package path spells, the error differs by OS.

