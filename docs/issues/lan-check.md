# LAN check

Design: [Modules](../design/02-engine-core.md#modules), `lan-check`. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

- CI's LAN check fails on macOS in each run since `60a6551`: after its restart, the server logs `input_never_applied` for slot 1's input stamped 300, with `next_tick` 226 and outcome `Early`, and the check finds no input of slot 1 stamped 300 in the server's log.

## Ready
