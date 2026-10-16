# Questions of the review plan

## Batch 9, item 5: a build into fog (production/src/placement.rs)

**Item.** A placement passes only where the builder's team sees every cell under its box, as Zero Hour refuses a build point the player does not see; D8 of design 11 and production.md change, and the client's preview refuses unseen ground from the same test.

**Why it needs a decision.** The server knows which cells each team sees only inside the Vision stage's `Fog`, a local of `Seeing::see`, and a predicting client computes no fog at all: its `ByHolding` sight says only that its team sees each unit it holds. The preview cannot run "the same test" until the client has the cells its team sees.

**Options.**

| Option | What it does | Cost |
| --- | --- | --- |
| A. The client computes its team's fog (recommended) | The fog of the server's Vision stage becomes a cache both roles build; the client builds it for its own team, from the sights of the units it holds, the map's brush and the reveals it receives | The client runs the sight maps each tick for one team; the reveals and the allied sights must reach it as the server holds them, or the two disagree at the edges |
| B. The server sends each team its seen cells | A per-team bitset of the grid's cells, sent on change | Up to 2²² cells, half a mebibyte a change at the largest map; a new replicated kind |
| C. The server alone refuses unseen ground | The server's placement test reads the fog cache; the preview keeps its present rule | The preview and the server disagree on fogged ground, which design 11 D8 then names |

**Blocked.** Item 5 alone. Item 4, the rules that read what the team sees over a spatial search, does not wait for it.
