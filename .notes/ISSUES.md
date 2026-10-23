# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

- **Navigation.** In the Zero Hour check's base play, GLA's tunnel defender #230 walks from (1194.6, −2175.1) toward (1076.1, −1768.8) stuck for 17,839 ticks, about 10 minutes, at 1,140 s.

## Ready

- **Net.** **Plan: Commanded units.** An order carries its units' ids against the session's bound on an input, 1,024 bytes on a LAN, so the client drops an order to more than about 500 units of 2-byte ids, or 335 of 3-byte ids, fewer as ids rise, and each order logs every id again ([The size of an order to a group](../docs/research/order-size.md)).
