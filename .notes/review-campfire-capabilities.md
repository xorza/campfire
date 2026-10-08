# Review: campfire-capabilities

Whoever addresses an item deletes it. A group whose items are all gone is deleted too.

Paths are relative to `source/crates/campfire-capabilities/src/` unless they name a crate. Groups are named after the root cause their items share, and sorted by severity and benefit across the whole crate: wrong behavior first, then untrusted-data robustness, then structure, then cleanup. An item marked **(bug)** was confirmed against the code: the behavior is wrong today.

Fix the root cause of a group, not its items one by one. Most groups give the structural target first, and their items are the places that target removes.

## 10. Script call plumbing is written once per caller

The same steps to begin a frame, call a hook within the budget, apply on success and record on failure are written in each system. Also, each capability applies its effects in a different shape. Target: one `ScriptBatch` (or `Calls`) method that runs a hook and does all of those steps, and one shape for `Effect::apply`.

- [ ] `combat/mod.rs` (8 free-function systems, `Wielded`, `GoingOff`, `IntervalDue`, the `RowParts` and `Attacker` aliases, about 620 lines) and `stats/mod.rs:263-302` (`expire_modifiers`, `clear_dead_modifiers`, `fill_row`), while `DamagePass`, `Refresh` and `HeldPass` are unit-struct namespaces in their own files. Two conventions for one job. Target: one type per system, in its own file. `mod.rs` keeps `install`, the sets and the module list. **Decided: crate-wide. Every capability's systems move into unit-struct files by concern, and each `fill_row` becomes a method of its column type; done after the other items, one capability per commit.**

## 15. A file holds several major types

The coding guide says one major struct per file, with the same name.

- [ ] `production/gather_loop.rs` (769 lines): `GatherLoop`, `GatherView`, `Step`, `Gather`, `Place`, `FoundNode`, `HeldNode` and `Checked`. `GatherView` has a second `impl` at `:701`. Target: `gather_loop/` with the view, the step and the systems in their own files.
- [ ] `production/construction.rs` (636 lines): `Construction`, `BuildView`, `BuildingAt`, `Step`, `Start`, `Building` and `Placed`. Target: `construction/`, split the same way.

## 16. The published surface names some types and not others

- [ ] `lib.rs:57-89` exports `ModeData`, `ModeParam`, `MapData`, `ModeInput`, `Books`, `ModeInputs`, `ModeSetup` and `Mode::install`. It does not export the `pub` types in their public fields and signatures: `ChoiceData`, `InputType` (in `ModeInput::decode`), `ListEntry` (the payload of `ModeParam::List`), `RelationData`, `GridData`, `BrushData`, `MapNavigationData`, `WallData`, `PathData`, `PlacedUnitData`, `MarkerData`, `RegionData`, `MapPoint`, `ModeUnits`, `ModeBooks` and `ModeMap`. A caller cannot name them, so it cannot match `ModeParam::List`, build a `ModeSetup` or `MapData`, or store the parts of `ModeInputs`. Target: one decision for the surface. Either export each of them, or make it `pub(crate)` and make the owner's field private.

## 17. Small code-guide slips

- [ ] `items/inventory.rs:52`, `items/item_book.rs:45`: `slots()` and `modifiers()` return `&self.field` through `Vec` deref, so they are not `const`. Target: `const fn` with `as_slice()`, as `ModeState::get` does.
- [ ] `mode/mode_book.rs:272-285` (`spawn_group`): the `Paths::waypoint` `expect` runs before the early return for empty `units`. Target: return first.

## 18. Stale or broken documentation

- [ ] `mode/mod.rs:106-111` (`Mode::install`): says that "in Mode, the trains whose time ended spawn". The systems added are `slot_events`, `run_timers`, `unit_deaths` and `level_ups`, and trains finish in `ProductionSet::Finish`.
- [ ] `actions/mod.rs:108-110` (`Actions::schedule`): says that it adds the start of every action in Act. It adds only `hold_charges` and `hold_passives`. The start is in `Abilities::install`.
- [ ] `stats/stats_api.rs:24-25`: says that the API covers "the planned crowd control and experience". Experience is in `progression` and runs.
- [ ] `actions/action_data.rs:306-320`: `charge_at` has the doc of `channel_at` ("Its channel at `rank`..") above its own. `channel_at` has none. `ChargesData` and `ChargeData` (`:119-130`) have no docs, and their names are almost the same.
- [ ] `actions/action_book.rs:105-111` (`check` doc): line breaks in the middle of a sentence. `abilities/abilities_api.rs`: no blank line before `fn origin`. `abilities/mod.rs:34-64`, `actions/mod.rs:10-34`: blank lines split the import groups at random.
