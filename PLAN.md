# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## A ready stepper has stepped

`net-stepper` alone, a batch.

### N1. `Stepper::init` steps the server once at least (net-stepper's stepper)

- [ ] With no client, `Stepper::init` returns before any step (`stepper/mod.rs:349`), so a stepper made `Init::Ready` holds a server that has never updated, and `integration/replication/send_insert_0_clients` times that first update with its setup, 432 µs where `send_insert_1_client` costs 96 µs → `init` steps until the server has run an update and each client is ready, so `Init::Ready` means the same with no client as with any. Builds: what can be forgotten is a type, `Init::Ready`'s promise kept by `init`, not by each caller. Takes the **Net** issue of the bench.
- Checks: a stepper of no client made ready has run its server's first update, as one of one client has; `send_insert_0_clients` costs no more than `send_insert_1_client`.

## Foundations
