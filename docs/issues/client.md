# Client

Design: [Modules](../design/02-engine-core.md#modules), `client`. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

## Ready

- The pointer takes the ground under the cursor where its ray meets the plane at height 0 (`Pointer::ground`), though a map with a heightmap draws its ground and its units at the heights `GroundHeights` gives, so a move order on such a map lands off the point clicked.
- The HUD shows no charges: an action with charges shows only its lockout as its cooldown, not how many charges it holds or when the next comes back.
- The client has no shop and no item keys: a player cannot buy, sell or use an item, which design 04's items give as orders.

- **Stage 8.** The client draws no area units: a unit with an `Area` is left out of the drawn units, and nothing shows where an area lies or how far it reaches.
