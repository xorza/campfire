# Review: campfire-capabilities

Whoever addresses an item deletes it. A group whose items are all gone is deleted too.

Paths are relative to `source/crates/campfire-capabilities/src/` unless they name a crate. Groups are named after the root cause their items share, and sorted by severity and benefit across the whole crate: wrong behavior first, then untrusted-data robustness, then structure, then cleanup. An item marked **(bug)** was confirmed against the code: the behavior is wrong today.

Fix the root cause of a group, not its items one by one. Most groups give the structural target first, and their items are the places that target removes.

## 10. Script call plumbing is written once per caller

The same steps to begin a frame, call a hook within the budget, apply on success and record on failure are written in each system. Also, each capability applies its effects in a different shape. Target: one `ScriptBatch` (or `Calls`) method that runs a hook and does all of those steps, and one shape for `Effect::apply`.

- [ ] `combat/mod.rs` (8 free-function systems, `Wielded`, `GoingOff`, `IntervalDue`, the `RowParts` and `Attacker` aliases, about 620 lines) and `stats/mod.rs:263-302` (`expire_modifiers`, `clear_dead_modifiers`, `fill_row`), while `DamagePass`, `Refresh` and `HeldPass` are unit-struct namespaces in their own files. Two conventions for one job. Target: one type per system, in its own file. `mod.rs` keeps `install`, the sets and the module list. **Decided: crate-wide. Every capability's systems move into unit-struct files by concern, and each `fill_row` becomes a method of its column type; done after the other items, one capability per commit.**

## 14. `BookBuilder` functions do too much

- [ ] `books/book_builder.rs:516-619` (`unit_type`, about 100 lines): AI, kit, production, walker, slot list and passive in one body that borrows `self.books` four times. Target: one method per part.
- [ ] `books/book_builder.rs:374-467` (`action`, about 95 lines): builds `BuildNames` twice with the same fields except `action` (`:382`, `:454`), and inlines the construct, requirement and params steps. Target: a `BuildNames` constructor that takes the action, and one method per step.
- [ ] `books/book_builder.rs:727-766`: `ActionNames` and `EffectNames` are both implemented for `BuildNames` with the same bodies for `damage_kind` and `modifier`/`unit_type`. `delivery_type` is the same as `standing_type` (`:759-765`). Target: one resolver each, or merge the traits.
- [ ] `books/book_input.rs:38-44`: `BookInput::damage_kind` looks up a name in `combat.damage_kinds`, which is a combat-rules lookup. Target: a `CombatRules` method beside `life_pool`.

## 15. A file holds several major types

The coding guide says one major struct per file, with the same name.

- [ ] `actions/action_slots.rs`: `InProgress` (with a 70-line impl), `OrderPhase`, `Started`, `ChannelStep`, `ChannelCall`, `ActionCall` and `SlotAim` are beside `ActionSlots`. Target: `in_progress.rs` (with the order, charge and channel satellites), `slot_aim.rs` and `channel_call.rs`. `ActionSlot` and `SlotCharges` stay.
- [ ] `actions/action_book.rs` (`Checked`, about 100 lines of geometry) → `checked.rs`. `actions/purse.rs` (`Payer`) → its own file, or merged as in group 2. `actions/effect_lists.rs` (`Does`, `Amount`, `LaunchId`, `ListsOf`, each with its impl) → own files. `actions/action_data.rs` (`Targeting` with its own `Deserialize`, `RankFields`, `RankChannel`, `RankToggle`, `RankCharges`, `TogglePer`, `RangeField`) → own files. `actions/slot_kinds.rs` (`SlotKindData`, `SlotRanks` with its own `Deserialize`) → own file.
- [ ] `production/gather_loop.rs` (769 lines): `GatherLoop`, `GatherView`, `Step`, `Gather`, `Place`, `FoundNode`, `HeldNode` and `Checked`. `GatherView` has a second `impl` at `:701`. Target: `gather_loop/` with the view, the step and the systems in their own files.
- [ ] `production/construction.rs` (636 lines): `Construction`, `BuildView`, `BuildingAt`, `Step`, `Start`, `Building` and `Placed`. Target: `construction/`, split the same way.
- [ ] `production/holdings.rs`: `Held` and `Holdings`. Target: one file each.
- [ ] `scripts/script_api/mod.rs` (479 lines): `ScriptApi`, `ApiMember`, `MemberKind`, `HookStatus`, `TagPropertyStatus` and `DataField`, and the 130-line Markdown writer `write_reference` (`:196-326`) in the registry. Target: one file per type, and the writer in its own module.
- [ ] `combat/mod.rs`: `Wielded` (with logic in `strikes`), `GoingOff` and `IntervalDue`. Target: `wielded.rs`, `going_off.rs`, `interval_due.rs` (see group 10 for the systems).
- [ ] `units/body.rs:23-31,105-160` (`BodyForm`, `Form`), `values/metric.rs:109` (`Approach`), `units/spawner.rs:14-22` (`SpawnAt`), `units/script_view.rs:106` (`View`, beside `ScriptView` and `CoreSource`). Target: `body_form.rs`, `approach.rs`, `spawn_at.rs`, `view.rs`.
- [ ] `mode/mode_setup.rs:35,46,55`: `UnitTypeSetup`, `SlotAction` and `LoadoutSetup` (with its own impl and tests). Target: their own files.
- [ ] `mode/mode_map.rs:46,58`: `MapGround` (with its own impl) and `MarkerSpec`. Target: their own files.
- [ ] `mode/map_data.rs:55-138`: `GridData`, `BrushData`, `MapNavigationData`, `WallData`, `PathData`, `PlacedUnitData`, `MarkerData`, `RegionData` and `MapPoint` (with its own impl) beside `MapData`. `PathData` has no doc. Target: at least `MapPoint` and `RegionData` in their own files.
- [ ] `books/mod.rs:63,92,165`: `BookParts` and `ModeInputs` are in `mod.rs` beside `Books`. `replace` is a free function called twelve times. Target: `book_parts.rs` and `mode_inputs.rs`, and `replace` as a method of its owner.

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
