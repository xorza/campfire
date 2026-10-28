# Zero Hour's portable structures and entries

What the released source (GeneralsX, read as a specification) and the install's `INIZH.big` say of the Overlord's and the Helix's upgrades and of a rider's walk to its container, and what a replay of a client match and a scripted match of the import showed. Read and measured 29 October 2026. Paths are under `GeneralsMD/Code/GameEngine/`.

## The upgrades

- **Three upgrades, one at a time.** `ChinaTankOverlord` holds an `ObjectCreationUpgrade` for each of `Upgrade_ChinaOverlordGattlingCannon`, `Upgrade_ChinaOverlordPropagandaTower` and `Upgrade_ChinaOverlordBattleBunker`, each `ConflictsWith` the other two, and a `CommandSetUpgrade` of each that swaps its command set: `ChinaTankOverlordDefaultCommandSet` shows the three upgrades at places 1, 3 and 5, and each upgrade's own set shows none of them, the bunker's showing `Command_TransportExit` at 1 to 5 and `Command_Evacuate` at 6. `ChinaVehicleHelix` holds the same three of `Upgrade_ChinaHelix…`, with a `WeaponSetUpgrade` of the Gattling cannon's and a `PassengersFireUpgrade` of the bunker's.
- **The carrier holds one structure.** `OverlordContain` is a transport of `Slots = 1` and `AllowInsideKindOf = PORTABLE_STRUCTURE`, `PassengersAllowedToFire = Yes`, which passes its transport queries to its first rider once full; `HelixContain` takes one portable structure past its `Slots = 5` of `INFANTRY VEHICLE` (`HelixContain::isValidContainerFor`), and lets it fire while its other riders fire only with `PassengersAllowedToFire` or the upgrade (`isPassengerAllowedToFire`).
- **The structures.** Each is `PORTABLE_STRUCTURE CLICK_THROUGH IGNORED_IN_GUI`, of `MaxHealth = 100`, drawn at its carrier's bone by `W3DDependencyModelDraw`'s `AttachToBoneInContainer`, `FIREPOINT01` on the Overlord and `CHASSISBONE` on the Helix:
  - The Gattling cannon, `NVOvrlrd_G` and `NVHelix_G`, fires `GattlingBuildingGun` and `GattlingBuildingGunAir` from a turret that pitches, and detects stealth while contained (`StealthDetectorUpdate`, `CanDetectWhileContained = Yes`).
  - The propaganda tower, `NVOvrlrd_S` and `NVHELIX_S`, heals each unit within 150 by 1 % of its most health a second, 2 % upgraded, every 2,000 ms (`PropagandaTowerBehavior`).
  - The battle bunker, `NVOvrlrd_B` and `NVHELIX_B`, is a `TransportContain` of `Slots = 5`, `INFANTRY`, `PassengersAllowedToFire = Yes`.
- **The Overlord's own Gattling weapon is a dummy.** Its `WeaponSetUpgrade` of the Gattling cannon gives it `GattlingBuildingGunAirDummy`: `PrimaryDamage = 0.0001`, `AttackRange = 400`, anti-air only, so the Overlord aims at aircraft while its rider fires.

## The walk to a container

- **The rider ignores its container.** `AIEnterState::onEnter` tells the pathfinder to ignore the container (`ignoreObstacle`) and lets the locomotor stand at an invalid position; `update` sets its goal to the container's position each frame, so the rider follows a container that moves, and gives up when the container is in another container above the terrain, or no longer takes it (`canEnterObject` with `CHECK_CAPACITY`).
- **A path that fails ends the state.** `AIInternalMoveToState` fails when no path reaches its goal, and the enter state with it, so the rider stands idle.

## The import, measured

- **The Overlord's upgrades.** The import of 29 October 2026 (`OUTPUT_VERSION` 85) gives `chinatankoverlord` the three researches at once in its command slots, its Gattling upgrade the dummy weapon alone, gated by `weapon_upgrade`, and its bunker the tag `upgrade_chinaoverlordbattlebunker_created`, which its entry's `open` tests; the Gattling cannon's and the propaganda tower's objects are dropped.
- **A client match.** The session `e578b1ec…` of Tournament Desert, China against the America builder bot from base starts, replayed on the server's log: the Overlord 171's bunker opened at tick 1,076, and at the match's end the server's entry check admitted each of the player's 17 Red Guards, Tank Hunters, Hackers and Black Lotus; the log holds no enter order of any unit to an Overlord. One Black Lotus was ordered into the Helix 165 at tick 3,530 and stayed approaching to the end; the Helix landed from 98 m to 21 m and rose again at tick 3,591, and took no rider.
- **A scripted match.** China's base start of the same map at 30 ticks a second, its 8 infantry ordered into a Helix that stood on the ground: 3 entered, at ticks 136, 229 and 291; the other 5 stopped 65 m to 145 m from it with no destination, their orders approaching until the play ended at tick 1,000. After the Overlord's bunker research, the same 8 ordered into it: 5 entered by tick 2,031 and 3 were refused, as its 5 places were full.
- **An approach that stops short.** `WalkItem::to` leaves a unit that arrived short of a goal standing with no destination until the static bodies change, and `Casts::start_casts` sets an approaching cast's walk each tick without asking whether it gave up, so its order stays approaching.
