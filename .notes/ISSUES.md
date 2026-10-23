# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

- **Navigation.** In the Zero Hour check's base play, GLA's tunnel defender #230 walks from (1194.6, −2175.1) toward (1076.1, −1768.8) stuck for 17,839 ticks, about 10 minutes, at 1,140 s.

## Ready

- **Import.** **Plan: Losses that show.** A spawned object whose unit type fails, as a locomotor no file defines, is dropped by `SupplyDropCause::NoObject`, though the object exists (`SupplyImport::walker`, `SupplyImport::free`).
- **Players.** **Plan: Losses that show.** An income or a channel's pay that would carry a player's amount past an `i64` is refused whole, and its caller discards the refusal (`let _ = resources.add(…)`), so the payment is lost with no trace.
- **Sim.** **Plan: Losses that show.** A command no addressee can decode, a payload with a flaw in `TickInputs`, a select, an order or a mode input, is dropped with no count and no log, so an encoder that disagrees with its decoder loses each order it writes and no test or check sees it.
