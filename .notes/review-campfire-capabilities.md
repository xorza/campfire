# Review: campfire-capabilities

Whoever addresses an item deletes it. A group whose items are all gone is deleted too.

Paths are relative to `source/crates/campfire-capabilities/src/` unless they name a crate. Groups are named after the root cause their items share, and sorted by severity and benefit across the whole crate: wrong behavior first, then untrusted-data robustness, then structure, then cleanup. An item marked **(bug)** was confirmed against the code: the behavior is wrong today.

Fix the root cause of a group, not its items one by one. Most groups give the structural target first, and their items are the places that target removes.

## 10. Script call plumbing is written once per caller

The same steps to begin a frame, call a hook within the budget, apply on success and record on failure are written in each system. Also, each capability applies its effects in a different shape. Target: one `ScriptBatch` (or `Calls`) method that runs a hook and does all of those steps, and one shape for `Effect::apply`.

- [ ] `combat/damage_weigher.rs`, `combat/heal_weigher.rs`, `combat/combat_events.rs`: three newtypes over `Box<dyn Fn(&mut ScriptBatch, X) -> R>`, each with the same `new`, call method and hand-written `Debug`. Target: one generic `ScriptFn<Arg, Out>`.
- [ ] `combat/mod.rs` (8 free-function systems, `Wielded`, `GoingOff`, `IntervalDue`, the `RowParts` and `Attacker` aliases, about 620 lines) and `stats/mod.rs:263-302` (`expire_modifiers`, `clear_dead_modifiers`, `fill_row`), while `DamagePass`, `Refresh` and `HeldPass` are unit-struct namespaces in their own files. Two conventions for one job. Target: one type per system, in its own file. `mod.rs` keeps `install`, the sets and the module list.
- [ ] `scripts/frame.rs:25-50,153-176`: `Frame` copies the ten fields of `CallStart` one by one, with one getter each, and `begin` destructures `CallStart` back into them. `pure` is the only `pub(crate)` field. Target: `Frame` holds the running call's `CallStart`, and `pure` gets a getter.
- [ ] `scripts/call_start.rs:25-85`: `cast` and `hook` each list all ten fields. `Frame::begin_think` already uses `..CallStart::mode(role)`. Target: struct update from `mode`.
- [ ] `scripts/api_builder.rs:153-232`: `index`, `index_in_call`, `index_set` and `index_set_in_call` repeat the same register-then-`forward_properties` body. Only the closure's `NativeCallContext` parameter is different. Target: one shared body.
- [ ] `mode/mode_api.rs:220,278,343`: the `mode` closure (`MemberSpec::call(..).roles(RoleSet::MODE)`) is defined three times. Target: one helper.
- [ ] `mode/mode_api.rs:493-498,501-575`: `ModeApi::choice(ctx, name)` fetches the `ModeBook` again, but `choose`, `chosen`, `offers` and `available` already hold it. Target: `choice(book, name)`, as `team(book, name)` does.

## 11. One script view type has unrelated jobs

- [ ] `units/script_view.rs:60-102` (`ScriptView`), `:345-807` (`View`): one struct holds the incremental unit-row reader (`core`, `sources`, `units`, `kept_units`, `seen`, `kept_seen`, `marks`, `reads`, `refill`, `columns`), the mode's names (`types`, `teams`, `paths`, `consts`, `resource_names`), the clock (`rate`, `now`), per-phase copies of resources (`relations`, `metric`, `bounds`), the target index (`bodies`, `indexed`, `found`) and the query API (`find`, `nearest`, `units_where`, `units_tagged`, `avatars`). `View` is a facade of about 60 forwarding methods over one `Rc<RefCell<..>>`. Target: separate the row reader, the name tables and the query layer. Each owns its state, so a name lookup does not borrow the reader, and the queries take plain inputs. The items below are parts of this split.
- [ ] `units/script_view.rs:218,223,294,546`: "the core is mark 0, and capability sources start at 1" is spelled as bare `0`, `at + 1` and `SOURCES - 1`. Target: one constant or `SourceIndex` newtype, and one mapping from source to mark bit.
- [ ] `units/script_view.rs:400-420` (`has_team`, `has_player`, `fits_resources`): "no mode set the teams" is `teams.count() == 0` on a default `Rc<Teams>` (`:63`, `:349`), and each check derives it again. Target: model the unset state once (`Option<Rc<Teams>>`, or a `Teams` method).
- [ ] `units/script_view.rs:710-783` (`find`, `nearest`), `units/units_api.rs:136`, `vision/vision_api.rs:58`: both queries repeat the negative-radius check, `index_bodies`, `Filter::parse`, the `visit_near` + `metric.reaches` + `filter.selects` + `seen` predicate and the `Unit::new(.., self.clone())` handle. The negative-radius guard is in four places. Target: one candidate visitor and one checked radius entry, shared with the position and vision APIs.
- [ ] `vision/vision_column.rs:48,54`: `sees` (with `ViewColumns`, `is_none_or`) and `can_see` (with `View::column`, `unwrap_or(true)`) both answer "does `team` see row `row`, true with no vision column". Target: `can_see` uses the same row test as `find` and `nearest`.
- [ ] `units/script_view.rs:456-481` (`window`, `ticks`, `lasting`, `duration`): tick conversions use only `rate`, but go through the view's `RefCell`. `ticks` returns `Checked<Ticks>`, and `lasting` returns `Result<Ticks, ApiError>`. Target: conversions on `TickRate`, with one error shape.
- [ ] `units/script_view.rs:500-535,599-640` (name accessors): id to name returns `Checked<Dynamic>` (`team_name`), `Dynamic` with unit for none (`path_name`, `unit_type_name`), or `ImmutableString` (`track_name`, `damage_kind_name`, `ability_name`). Name to id returns `Result<Tag, ApiError>` (`tag_named`), `Checked<DamageKind>` (`damage_kind_named`), or `Option` (`resource_named`, `path_named`, `unit_type_named`). Target: one return shape per direction, and the script wrapping done once at the binding.

## 12. One concept has several types or names

- [ ] `values/region.rs:1-26` and `values/bounds.rs:10-60`: both are a closed `[x, z]` rectangle with `contains(Position)`. `Bounds::new` validates, and `Region::new` asserts. A third `Region` struct is in `navigation/regions/mod.rs:56`. Target: one rectangle type with a checked constructor, and a different name for the navigation one.
- [ ] `values/body_box/mod.rs:36-40` (`BoxApproach`) and `values/metric.rs:109-114` (`Approach`): the same struct `{ nearest: Ordering, share: Fraction }`, converted field by field in `Metric::meets` and `Shape::comes_within`. Target: one `Approach` in its own file, which `BodyBox::approach` returns.
- [ ] `values/shape.rs:56-72` (`Shape::comes_within`), `values/metric.rs:97-116` (`Metric::meets`), `navigation/segment/mod.rs:38-43` (`Segment::comes_within`): the circle case `Approach::of(path, off, reach + radius).nearest == Less` is written three times. `meets` and `comes_within` add `reach + radius` without a check, but `Metric::reaches` uses `checked_add`. Target: one `Shape` method with one overflow rule.
- [ ] `values/grid/mod.rs:73,148,191,250`, `values/polygon/mod.rs:19,120`, `values/body_box/mod.rs:13-14`: the "Num in halves of a bit" conversion is a closure defined three times in `Grid` and a free function in `polygon`. The `[i128; 2]` point is declared as `Flat` and as `Twice`, and again as the return type of `Grid::center_twice`. Target: one point type with one constructor.
- [ ] `values/grid/mod.rs:141-183` (`box_spans_closer`) and `:185-223` (`box_covers`): the same run-collecting loop. Only the span bounds and the predicate are different. Target: one helper that takes the span and the predicate.
- [ ] `values/body_box/mod.rs:84-86` (`BodyBox::MAX_REACH`, 64 m) and `units/body.rs:40` (`Body::MAX_RADIUS`, 64 m): one bound in two constants that must stay equal. Target: one constant.
- [ ] `values/relation.rs:5-12`, `values/attitude.rs:5-15`, `values/engine_enum.rs:5-10`, `values/stat.rs:11-16,42-47`: the script enum `Relation` (`EngineEnum::Relation`, doc `Relation::Hostile`) is the Rust type `Attitude`, and the Rust type `Relation` is the filter selector (enemies, hostiles, neutrals, allies, all). Target: one name per concept in Rust and in scripts.
- [ ] `actions/action_book.rs` (`Checked<'a>`, the action-check result), `scripts/error.rs` (`Checked<T>`, the script result) and `production/gather_loop.rs` (`Checked`): three types with the same name. `actions_column.rs`, `abilities_api.rs` and `areas_api.rs` import the script one beside the action one. Target: rename the two that are not the script result (for example `CheckedAction`).
- [ ] `actions/range.rs` (`Range`) against `std::ops::Range`: `actions_column.rs` imports both and writes `range::Range`. Target: rename (for example `Reach`).
- [ ] `actions/actions_effect.rs` (`ActionsEffect`): only a listed `spawn`. Its siblings are named for what they do. Target: `SpawnEffect`.
- [ ] `mode/calls.rs:65,76`: `Calls::weigh` (damage) and `Calls::weigh_heal`. Target: `weigh_damage` and `weigh_heal`.
- [ ] `players/resource_id.rs:16`, `items/item_id.rs:15`, `stats/pool_id.rs:24`: `ResourceId::named(&[DeclaredName], &str)`, `ItemId::named(&BTreeMap, &str)` and `PoolId::named(&BTreeMap, &DeclaredName)` take the name in different types. `mode/mode_data.rs:204-205` passes `name` to one and `name.as_str()` to another. Target: one name type across the id constructors.
- [ ] `units/action_id.rs:10`, `modifier_id.rs:10`, `path_id.rs:9`, `layer.rs:15`, `track_id.rs:17`, `tag.rs:15`, `unit_type.rs:15`, `team.rs:22`, `values/damage_kind.rs:14`: the index newtypes differ in constructor name (`nth`, `new`), argument width (`u32`, `usize`, `u8`, `u16`), failure (`TrackId::new` returns `Option`, `ModifierId::nth`, `PathId::new` and `Tag::new` panic, the rest cast without a check) and `const`. Target: one constructor name and one policy (checked narrowing in the book that numbers them, infallible `new` from the stored width), and one `index()` return type.
- [ ] `units/unit_state_book.rs:36-52` (`fields`, `field_named`), `stats/param_table.rs:93`: each caller of `NameTable` tests `has_run(run)` before `values` or `named`, because those index `starts[run + 1]` and panic out of range. `get_named` has the same panic. Target: accessors that answer for a missing run (empty slice, `None`).
- [ ] `scripts/script_consts.rs:10-47`: four parallel `Vec<ImmutableString>` tables, each with a `set_*` and a getter. Target: one name-table type, one instance per id kind.
- [ ] `units/body.rs:49-60,72-78,112-136`: `Body::boxed(BodyBox)` and `BodyForm::boxed([Num; 2])`, and `Body::radius` (`pub`) and `BodyForm::radius` (`pub(crate)`): the same names with different arguments and visibility. Target: names that say whether they take a form or a built body.

## 13. A fact is derived in several places in one module

- [ ] `production/gather_loop.rs:244,347,747`: "a drop-off of this owner that takes this resource" is spelled in `drop_off()`, in the `chosen` closure of `step`, and in `resolve`. Target: one `GatherView` method.
- [ ] `production/construction.rs:162,576`, `production/gather_loop.rs:169`: `book.range(slots, slot)` then `let Range::Meters(range) = .. else { panic!(..) }`, three times. Target: one `ActionBook` accessor that gives meters for a slot.
- [ ] `production/construction.rs:264-276,576-580`: `BuildView::in_range` and the inline `metric.reaches(..)` in `progress_sites` test the same rule. Target: `progress_sites` uses `in_range`.
- [ ] `production/site.rs:119-127`, `production/construction.rs:609-611`: a build's time is read from `windup` and converted to `Num` in two ways. Target: one method.
- [ ] `production/supply.rs:35-61`, `production/production_column.rs:91-109`: a player's `used`, `given` and capped total are summed for the train check and again for the script view. Target: one type sums and caps, and the column feeds it.
- [ ] `production/supply.rs:46-50,77-82`: the "grow `players` to hold slot `at`" resize is in `count` and in `reserve`. Target: one private `entry_mut(player)`.
- [ ] `production/gather_loop.rs:202,266`, `production/mod.rs:310`: `GatherView::distance` and `GatherView::walk` take no `self` and work on `Place`. `Production::ordered` takes only `ActionSlots` and `ActionBook`. Target: methods of `Place`, and of `ActionSlots` (with the book).
- [ ] `projectiles/mod.rs:76,86,118,143,294`: the fresh `Flight::Homing { target, flown: ZERO, lost: false }` is written three times, and the fresh `Flight::Line` twice. Target: `Flight::homing` and `Flight::line`.
- [ ] `projectiles/mod.rs:100-107,150-163`: `deliver` fetches the action and the `ProjectileSpec`, then `range()` fetches both again. Target: `range` takes what `deliver` read.
- [ ] `projectiles/launches.rs:17`, `projectiles/mod.rs:189,289,316,325`: `Launches.launches` is a `pub(crate)` field that `push`, `take_shots` and `launch` change directly, beside the methods `cast` and `clear`. Target: a private field and methods.
- [ ] `orders/mod.rs:186,355`, `orders/unit_order.rs:55`: the `Ordered` tuple, `OrderedUnit` and the positional conversion in `Orders::ordered` describe the same nine parts. Target: `OrderedUnit` is the query (a `QueryData` derive), with no tuple and no conversion.
- [ ] `orders/mod.rs:318-337`: the slot-kind lookup `slots.and_then(..).and_then(|held| book.get(held.action?)).map(|action| action.kind.kind())` is in the `Slot` arm and in the `Build` arm. Target: one method on `ActionSlots` or `ActionBook`.
- [ ] `orders/ai.rs:30`, `orders/mod.rs:161`: `Ai::of` computes `thinks`, then calls `Orders::ai_period`, which converts the period and tests `thinks`. `Ai::of` is its only caller. Target: both steps in `Ai::of`, and `ai_period` removed.
- [ ] `orders/mod.rs:705` (`think`): matches `get_mut::<NextThink>` to overwrite or insert. `insert` does both. Target: one `insert`.
- [ ] `navigation/mod.rs:221,244,406,795`, `navigation/navigation_effect.rs:230`, `navigation/route_asks.rs:345` (`RouteAsks::walkable`), `mode/map_data.rs:208`: each spells `Walkable { clearance, statics, short: None }`. Target: one `Walkable::of(clearance, statics)`, and `RouteAsks::walkable` removed.
- [ ] `navigation/mod.rs:509-510,665-666`: `tags.is_some_and(|tags| tags.tags.contains(EngineTag::Gathering.tag()))` twice, beside `UnitTags::properties_of(Option<&UnitTags>)`. Target: `UnitTags::gathers(Option<&UnitTags>)`.
- [ ] `navigation/statics_dirty.rs:71,86`: `state_inserted` and `state_removed` have the same body. Target: one private function.
- [ ] `navigation/collider/mod.rs:55,88,104`: `overlaps`, `overlap` and `part` each derive "can these two separate" again. Target: one `Collider::may_part(&self, other)`.
- [ ] `navigation/pathing_grid/mod.rs:162,170`: `serving` binary-searches the kinds, then `clearance` searches again with `expect`. Target: one search that returns the index.
- [ ] `navigation/route_planner/mod.rs:85,87`: `Planned::cost` and `Planned::expanded` are computed for each plan, but only `route_planner/tests.rs` reads them. Target: remove them from the production result, or gate them for tests.
- [ ] `vision/sight_maps/mod.rs:99,115`: `sees` and `sees_any` (in a second `impl SightMaps` block) repeat the detection-or-reveal bitmap choice. `sees(group, cell, hidden)` is `sees_any(group, cell..cell + 1, hidden)`. Target: one method on a range in one `impl` block.
- [ ] `actions/action_slots.rs:173-208` (`InProgress::slot`, `slot_mut`): the same four-arm destructuring twice. Target: one place names each variant's slot.
- [ ] `combat/damage_handle.rs:30-48,80-90`, `combat/heal_handle.rs:28-44,53-60`: the `source`, `target` and `ability` bindings are the same closures twice. Target: shared helpers.
- [ ] `combat/damage_pass.rs:390,397,411`: `heal` and `restore` are `pub(crate)` but only `damage_pass.rs` calls them. `heal_living` is a wrapper with one caller. Target: private, with `heal_living` folded in.
- [ ] `actions/effect_data.rs:107-199,281-326`: nine `*Fields` structs mirror the `Effecting` variants, and the `Deserialize` impl maps them field by field (45 lines). Target: the variants hold the field structs, or deserialize through a tagged form.
- [ ] `mode/roster.rs:34-48`: `holds` and `ids` each repeat `match offers { Avatars => len, Loadout => len }`. Target: one `count(offers)`.
- [ ] `units/unit_types.rs:144-165` (`UnitTypes::tag_book`): a `&mut self` getter that `mem::take`s each type's tags into the `TagBook`. After it, `UnitTypes` holds empty tags, and a later `give_tag` is lost silently. Today the only production call (`mode/mode_books.rs:107`) comes after the last `give_tag`, so this is a trap for later code, not a bug now. Target: a load-time builder consumed by `tag_book(self)`, so the staged tags have one owner.

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
