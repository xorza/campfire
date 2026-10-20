# Zero Hour supply

How Zero Hour gathers its money, read on 20 October 2026 from GeneralsX's `GeneralsMD` source and the shipped INI of `INIZH.big`; and how OpenRA's harvester does it, for comparison.

## The loop

- **A dock gives a box at each action.** `SupplyWarehouseDockUpdate::action` gives the docked gatherer one box and returns true while the gatherer takes one; `AIDockProcessDockState` calls it once each `SupplyWarehouseActionDelay` frames from the gatherer's arrival, until the gatherer is full (`MaxBoxes`) or the dock is empty. A dock of `DeleteWhenEmpty` goes as its last box goes; one without stays, empty.
- **A center pays every box at once.** `SupplyCenterDockUpdate::action` empties the gatherer in one action, `SupplyCenterActionDelay` frames after its arrival, and deposits `ValuePerSupplyBox` for each box (`Player::getSupplyBoxValue`), plus `UpgradedSupplyBoost` with its upgrade.
- **The choice of a dock.** `ResourceGatheringManager::findBestSupplyWarehouse` takes the gatherer's preferred dock, the last a player's order named, while it can use it; else the nearest dock by center distance within `SupplyWarehouseScanDistance` of the gatherer, that is not empty, not an enemy's and clear to approach. `findBestSupplyCenter` takes the preferred or the nearest center of the gatherer's own player, at any distance (`ActionManager::canTransferSuppliesAt`: not an ally's). A computer player's gatherer scans twice its `SupplyWarehouseScanDistance` (`SupplyTruckAIUpdate::getWarehouseScanDistance`).
- **Autopilot.** `SupplyTruckAIUpdate`'s states: idle, busy (a player's move took it off), wanting (it docks at the best dock or center), regrouping (none found: it goes to the nearest supply center, command center or structure, and waits), docking. A gatherer that leaves a supply center's exit is forced into wanting (`SupplyCenterProductionExitUpdate`), so it starts at once; one whose order a player gave keeps that dock as its preferred one.
- **One docker at a time.** A dock serves one gatherer; the others wait at its `NumberApproachPositions`, -1 for any count.
- **A free gatherer.** Each supply center spawns one gatherer as it is built, through its exit (`SpawnBehavior`, `OneShot`).

## The shipped numbers

`ValuePerSupplyBox = 75` in `GameData.ini`; the source's default is 100. Times in milliseconds; at 30 frames a second, 1,000 ms is 30 frames.

| Gatherer | Trained at | Cost | `MaxBoxes` | Dock delay | Center delay | Scan |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `ChinaVehicleSupplyTruck` | China supply center | 600 | 4 | 1,000 | 400 | 700 |
| `AmericaVehicleChinook`, a helicopter | USA supply center | 1,200 | 8 | 1,200 | 2,900 | 700 |
| `GLAInfantryWorker`, who also builds | GLA command center, supply stash | 200 | 1 | 150 | 150 | 700 |

| Dock | `StartingBoxes` | `DeleteWhenEmpty` | `NumberApproachPositions` |
| --- | ---: | --- | ---: |
| `SupplyDock` | 400 | no | -1 |
| `SupplyWarehouse` | 400 | no | 9 |
| `SupplyPile` | 150 | yes | 5 |
| `SupplyPileSmall` | 50 | yes | -1 |

The centers: `AmericaSupplyCenter` (2,000), `ChinaSupplyCenter` (1,500) and `GLASupplyStash` (1,500), each spawning its faction's gatherer. A map's docks are objects of no player.

## OpenRA

`Harvester` in `OpenRA.Mods.Common` (branch `bleed`, read on 20 October 2026): `SearchOnCreation`, true by default, makes a new harvester search for resources; `SearchFromProcRadius` and `SearchFromHarvesterRadius` bound its searches; a harvester unloads `BaleUnloadAmount` bales each `BaleUnloadDelay` ticks.
