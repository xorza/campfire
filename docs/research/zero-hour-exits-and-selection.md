# Zero Hour's exits, docks, bunkers and selection

What the released source (GeneralsX, read as a specification) and the install's `INIZH.big` say of how a trained unit leaves its factory, how a gatherer docks, how an Overlord or a Helix carries a bunker, and how a player selects units; and what the engine and its import did with them. Read 28 October 2026. Paths are under `GeneralsMD/Code/GameEngine/Source/GameLogic/` but the selection's, under `Core/GameEngine/Source/GameClient/`.

## Exits

- **A unit is made inside its factory and drives out.** `DefaultProductionExitUpdate::exitObjectViaDoor` puts the new unit at `UnitCreatePoint`, in the factory's frame and on the ground, facing as the factory does, then sends it along an exit path to `NaturalRallyPoint` and on to the player's rally point when one is set, `aiFollowExitProductionPath`, which ignores the factory's own body as an obstacle (`Object/Update/ProductionExitUpdate/DefaultProductionExitUpdate.cpp`). `QueueProductionExitUpdate`, the barracks', makes its unit at its create point the same way.
- **The engine spawns in the nearest open cell.** `TrainPoints::spawn_point` spawns a trained unit at the centre of the open cell nearest the exit point, outside every body, and sends it to the rally point (`production/src/trains.rs`). A create point inside the factory's body, as the War Factory's and the barracks' `X:0 Y:0` are, puts the unit in whichever cell beside the body lies nearest, on any side.

## Docks

- **A dock is three bones and its waiting places.** `DockUpdate` reads the dock's `DockStart`, `DockAction` and `DockEnd` bones and `NumberApproachPositions` bones `DockWaiting<n>` (`Object/Update/DockUpdate/DockUpdate.cpp`): a gatherer drives in at the start, docks at the action point for its action, and leaves by the end; the others wait at the approach places.
- **The engine docks beside the box.** A gather reaches 10 m from a dock's or a center's body, the pathfinder's cell, from any side (09-zero-hour.md, A gatherer docks as a dozer builds), and its waiting gatherers stand at its side (09-zero-hour.md, Supply).

## Bunkers

- **The Overlord carries one portable structure.** `ChinaTankOverlord` holds an `OverlordContain` of `Slots = 1`, which, once full, passes transport queries to its first rider; `Upgrade_ChinaOverlordBattleBunker` runs `OCL_OverlordBattleBunker`, which makes a `ChinaTankOverlordBattleBunker` inside it (`ContainInsideSourceObject = Yes`), a `PORTABLE_STRUCTURE` of a `TransportContain` of `Slots = 5`, `AllowInsideKindOf = INFANTRY` and `PassengersAllowedToFire = Yes`, drawn at the Overlord's `FIREPOINT01`. The Gattling cannon and the propaganda tower ride the same way.
- **The Helix carries troops and a portable structure.** `ChinaVehicleHelix` holds a `HelixContain` of `Slots = 5`, `AllowInsideKindOf = INFANTRY VEHICLE PORTABLE_STRUCTURE`, `ForbidInsideKindOf = AIRCRAFT` and `PassengersAllowedToFire = No`; its battle bunker, Gattling cannon and propaganda tower conflict, so it carries one of them.
- **The engine has neither.** The import gives the Overlord and the Helix no `enter`, so no unit enters them, upgraded or not (09-zero-hour.md, Helicopters; 09-zero-hour.md, Transports and garrisons).

## Selection

- **A drag selects no structure.** `SelectionTranslator` refuses a structure to a drag (`KINDOF_STRUCTURE` while `dragSelecting`), as several buildings selected at once would confuse the command panel (`MessageStream/SelectionXlat.cpp`).
- **Shift adds and toggles.** While the prefer-selection key, shift, is held (`MSG_META_BEGIN_PREFER_SELECTION`), a click or a drag adds to the selection; a click on units that are all selected deselects them.
- **A double click selects its kind.** A double click on a unit the player controls selects every unit of its type on the screen (`selectMatchingAcrossScreen`), across the map with alt held (`selectMatchingAcrossMap`); with shift it keeps the units already selected; a structure, which no mass selection takes, gets none.
- **The engine.** A click selects one unit alone and a drag every unit of the player's whose ground point lies within its rectangle, structures among them; no key adds, and a double click is two clicks (`client/src/selecting.rs`).

## Sounds and ranges

- **A gated weapon has no look.** The import names a weapon of a set an upgrade chooses `weapon_<name>_starting` or `weapon_<name>_upgraded` (`combat_import/weapon_gate`), and `EffectsImport::action_looks` looks each weapon's converted action up by `weapon_<name>`, so none of the 39 gated weapons of the import, the Inferno Cannon's among them, has a fire sound or a fire effect. `InfernoCannonGun`'s `FireSound` is `InfernoCannonWeapon`, of `vinfweaa`, `vinfweab` and `vinfweac` in the base game's `Audio.big`.
- **The minimum range holds.** `InfernoCannonGun` gives `MinimumAttackRange = 50` against an `AttackRange` of 300, and the import gives the action its `min_range = 50`; the engine refuses an attack within it, edge to edge, and walks its attacker away to halfway between the minimum and the range (`core/actions/src/targets.rs`), as `Weapon::isWithinAttackRange` measures from the bounding spheres.

## The crash

- **A use order panicked the client.** In a China match against the America builder bot on Tournament Desert, about tick 5,750, `UseOrders::ordered` failed to read `(StableId, SlotsMut)` of a unit that `Using` had found a use or an entry for: `QueryDoesNotMatch` (`interaction/src/use_orders.rs:51`). `Using`'s query reads the unit's `ActionSlots`, which require `SlotTimes` and `ActionProgress` on any world that registers the requirement; the unit lacked one of those or its `StableId`. The session's journal is kept in the client's data; the bug report before it is of tick 4,950.

## A base start

- **A base start can enclose a unit.** The imported mode's `on_match_start` places a base's buildings, then its units, each at the room nearest the slot's rally within 40 cells (`packages/zero-hour/scripts/mode.rhai`); `NavigationColumn::room_near` takes the nearest cell clear of every body, with no test that the cell connects to the rest of the map, so America's first Ranger on Tournament Desert stands where its base's buildings enclose it. The release game has no base start: it is the mode's.
