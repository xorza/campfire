# Store

Design: [Storage](../design/02-engine-core.md#storage). Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

- **Plan: F9.** `DataDir`, `DurableFile` and `SecretFile` each branch on the platform with `#[cfg(unix)]`, and on Windows each branch does nothing, so a promise of `store` holds on Unix only: a file or directory made owner-only on Unix gets its parent's ACL on Windows; a secret file that other users may read loads on Windows, where on Unix it is refused ([Sessions](../design/10-sessions.md#decisions), D8); and a rename's directory is synced on Unix only, so on Windows a crash may lose a durable file's name (D9).

## Ready
