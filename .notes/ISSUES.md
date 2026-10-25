# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

## Ready

- **Net.** **Plan: N1.** `integration/replication/send_insert_0_clients` times the server app's first update: with no client, `Stepper::init` returns before any step, so the case holds the first update's setup and costs 432 µs where `send_insert_1_client` costs 96 µs (2026-10-25, local).
