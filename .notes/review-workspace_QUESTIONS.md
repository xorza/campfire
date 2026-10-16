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

## Batch 18, item 2: the engine's types write the import's data files (import, mode, capabilities)

**Item.** `units.toml`, `mode.toml` and the manifest are written by structs that mirror the engine's types by hand, and by `format!` templates; the engine types are to derive `Serialize`, and the importer is to build them, as it builds `MapData`. Batch 18 says no fingerprint moves.

**Why it needs a decision.** A TOML serializer writes a struct's fields in their order, values before tables. The engine's `UnitTypeData` orders `tags` before `pools`, and its tables `slots`, `stats`, `combat`, `collision`, `production`; the importer's mirror writes `pools` before `tags`, then `stats`, `collision`, `combat`, `slots`, `production`. Built from the engine's types, every unit type's section of `data/units.toml` changes its bytes, so the imported package's fingerprint moves, which a run on Linux and a run on macOS then pin again (design 12, D4). The fingerprint is already in the issue log, as it moved before this plan's batch 15.

**Options.**

| Option | What it does | Cost |
| --- | --- | --- |
| A. Do it, and pin the fingerprint again (recommended) | The engine's types derive `Serialize`; the importer builds them; `FINGERPRINTS` takes the new value of a run on Linux and one on macOS | The batch's rule of no fingerprint move breaks for this item; the macOS run is the user's |
| B. Reorder the engine's fields to the importer's | The bytes stay as they are | The engine's types take an order chosen by one importer's past output |
| C. Keep the mirrors | Nothing changes | The hand-kept mirrors stay, which the item names |

**Blocked.** Item 2 alone. Items 3 to 12 of batch 18 do not wait for it.

## Batch 19, item 5: where the match's mode lives (mode, script)

**Item.** `Ctx` had a `dyn Any` slot for the mode alone, and the mode kept its `ModeSpec` three times: the slot, the `MatchMode` non-send resource and its `ModeCall` part of the frame. The item asks that the mode read its spec through its `ModeCall` part, and that the slot and `MatchMode` go. The slot is gone: the mode's calls read the spec through `ModeCall`, by the new `ModeCtx` trait on `Ctx`.

**Why it needs a decision.** Thirteen readers of `MatchMode` are not calls: the run conditions of the mode's hooks, the declarations of the match entity's parts, the checks of `Choices`, `ModeState` and `UnansweredSlotEvents`, `ModeWorld::team_of` and `mode_state`, which the server's door reads, and `Mode::start`. Without `MatchMode`, each reads the spec through `world.non_send::<Ctx>().frame()`, a `RefCell` borrow of the call frame. A reader that runs while a call holds the frame, as an effect that applies and asks for a player's team, then panics on a double borrow, where `MatchMode` cannot.

**Options.**

| Option | What it does | Cost |
| --- | --- | --- |
| A. Keep `MatchMode` (recommended) | The world's readers keep the non-send resource, the calls read `ModeCall`; the spec is held twice, each by one kind of reader | Two handles of one `Rc<ModeSpec>` |
| B. Remove `MatchMode` | Every reader goes through the frame's `ModeCall` part | A world-level read becomes a borrow of the call frame, which panics if a call holds it |
| C. Make `ModeSpec` itself the non-send resource | `MatchMode`'s `Deref` wrapper goes, and the world holds the `Rc<ModeSpec>` by a resource type of its own | A newtype is still needed for an `Rc` in a non-send resource, so little changes |

**Blocked.** The removal of `MatchMode` alone.

## Batch 19, item 6: one text type for state and scripts (script)

**Item.** Each read of a text state field makes a new `ImmutableString`, and each write a new `Arc<str>`; the item asks for one shared text type for state and scripts.

**Why it needs a decision.** The two sides need different sharing. Rhai builds without its `sync` feature, so its `ImmutableString`, which `ScriptText` holds, is an `Rc`, and is not `Send`. `StateValue` lives in Bevy components (`ModeState`, a unit's state), which must be `Send` and `Sync`, so its text is an `Arc<str>`. No one type is both, and a read or a write that crosses between them copies the text.

**Options.**

| Option | What it does | Cost |
| --- | --- | --- |
| A. Keep the copy (recommended) | Each crossing copies the text once, as now | One allocation for each text field a call reads or writes |
| B. Turn on Rhai's `sync` feature | `ImmutableString` becomes an `Arc`, which `StateValue` can hold | Every value a script holds must then be `Send` and `Sync`; `View`, `Ctx` and the handles hold `Rc` and `RefCell`, and each Rhai value's count becomes atomic |
| C. A match-wide table of texts | `StateValue::Text` holds an index into a table of each text as both types | The table grows with every distinct text a script makes, and is state a snapshot must hold, in a fixed order |

**Blocked.** Item 6 alone.

## Batch 20, item 2: orders asks other capabilities through contract parts (orders, navigation, production, units)

**Item.** The orders effect writes navigation's `Route`, `Progress` and `PathWalker`, and production's `Builder` and `Gatherer`. The item asks that orders ask for this work through contract parts of `units`. Its other parts are done: the effect is `OrdersEffect`, `UnitOrder::apply` is `UnitOrder::give`, and `Route::ask` takes a `RouteAsk` and restarts the walker's `Progress`.

**Why it needs a decision.** Design 02, Contracts, says that "`orders` keeps its dependency on `production`, whose build and gather orders a unit takes". So the design allows the production writes that the item removes. A contract part also moves each write from the order's effect to a system of its owner, which must run between the orders' stage and the first read, or the goldens move.

**Options.**

| Option | What it does | Cost |
| --- | --- | --- |
| A. Contracts for navigation alone (recommended) | `units` gains a path order (leave or rejoin) and a group ask on `Destination`, which navigation pursues; production's orders stay, as design 02 says | A design step for navigation's two parts, and a check that no golden moves |
| B. Contracts for both | Production's `Builder` and `Gatherer` take their orders from a part of `units` too; design 02's sentence changes | Two more parts and systems, and the same check |
| C. Keep the writes | Orders writes the parts as now | The Direction's contract rule stays broken for navigation |

**Blocked.** The contract part of item 2 alone.
