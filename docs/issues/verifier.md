# Verifier

Design: [Modules](../design/02-engine-core.md#modules), `verifier`. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

## Ready

- **Plan: F7.** The binary's `verify` in `main.rs` returns `Box<dyn Error>`, where [Code](../../AGENTS.md#code) asks for an error enum of cases: a caller cannot tell a package store that does not scan, a log that does not read or decode, a replay that does not start and a snapshot that does not check apart.

