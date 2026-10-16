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

## Batch 17, item 8: typed names in the import's report (import/src/zero_hour/mod.rs)

**Item.** `dropped_waypoints`, `missing_textures`, `unliving` and `wounded` hold names as strings; the item asks for typed names. Its other two parts are done: each map's entry of `Imported::maps` holds both its untextured cells and its dropped waypoints, one rule for every map, and a `PlacementViewAngle` that is not a number is refused as the INI reads it.

**Why it needs a decision.** Each of these names is an identifier the game's own files define: a waypoint's name in a map, a texture's file name as a W3D mesh writes it, an INI object's name. CLAUDE.md keeps strings "for human text and identifiers an outside format defines", and the report is printed for a person to review against the original. The item and that rule disagree.

**Options.**

| Option | What it does | Cost |
| --- | --- | --- |
| A. Keep the strings (recommended) | The report names the game's identifiers as the game spells them, as CLAUDE.md allows | The item's first part stays undone, and the item leaves the plan |
| B. One public newtype for each kind | `WaypointName`, `TextureName` and `ObjectName`, each the game's spelling, with `Display` | Three public types that hold no rule a string lacks |
| C. The engine's names where they exist | `unliving` and `wounded` name the unit types the import made (`DeclaredName`); waypoints and textures stay strings | A reviewer looks an object up by its engine name, not by the INI's |

**Blocked.** The typed-names part of item 8 alone.
