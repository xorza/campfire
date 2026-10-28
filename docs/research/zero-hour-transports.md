# Zero Hour's transports and garrisons

What the released source (GeneralsX, read as a specification) and the install's `INIZH.big` say of units that ride in a transport or garrison a building, and what OpenRA does. Read 28 October 2026. Paths are under `GeneralsMD/Code/GameEngine/`.

## The modules

- **The contain modules of the install's objects.** `GarrisonContain` 200, `TransportContain` 73, `HealContain` 15, `HelixContain` 13, `TunnelContain` 12, `RiderChangeContain` 11, `OverlordContain` 10, `ParachuteContain` 7, `InternetHackContain` 4, `RailedTransportContain` 1.
- **The three transports.** `AmericaVehicleHumvee`: `TransportContain`, `Slots = 5`, `PassengersAllowedToFire = Yes`, `DamagePercentToUnits = 100%`, `AllowInsideKindOf = INFANTRY`, `ExitDelay = 250`, `NumberOfExitPaths = 3`, `GoAggressiveOnExit = Yes`. `ChinaVehicleTroopCrawler`: `Slots = 8`, `InitialPayload = ChinaInfantryRedguard 8`, `HealthRegen%PerSec = 10`, `DamagePercentToUnits = 10%`, `AllowInsideKindOf = INFANTRY`, `ExitDelay = 250`, its passengers not allowed to fire. `AmericaVehicleChinook`: `Slots = 8`, `DamagePercentToUnits = 100%`, `AllowInsideKindOf = INFANTRY VEHICLE`, `ForbidInsideKindOf = AIRCRAFT HUGE_VEHICLE`, `ExitDelay = 100`.
- **A rider's size.** A transport counts each rider's `TransportSlotCount` against its `Slots` (`TransportContain::getContainMax`, `m_extraSlotsInUse`); the objects give 1 (227), 3 (198), 0 (168), 10 (18), 8 (15), 5 (10) and 2 (6), as a Ranger and a Rebel 1, a Crusader and a Humvee 3. A `GarrisonContain` counts its riders against its `ContainMax`, one each.
- **The garrisons of the factions.** `ChinaBunker`, `AmericaFireBase` and `GLAPalace` and each general's copy hold a `GarrisonContain`; the others are civilian, as `GermanCastle` of `ContainMax = 10`.

## Who enters

- **The module's tests.** `OpenContain::isValidContainerFor` takes a rider of one of `AllowInsideKindOf` and of no `ForbidInsideKindOf`, of a relationship to the container that its `AllowAlliesInside`, `AllowEnemiesInside` and `AllowNeutralInside` allow, each `Yes` by default (`OpenContainModuleData`'s constructor). `GarrisonContain` adds a living container not `REALLYDAMAGED` unless `GARRISONABLE_UNTIL_DESTROYED`, a rider not `NO_GARRISON`, and room for one more.
- **The action's tests.** `ActionManager::canEnterObject` refuses a rider that is the container, a dead container, one shrouded to the rider, either under construction, a sold container, a structure or an immobile rider, and a subdued container; a container of another player than the rider's when it holds a rider that is not stealthed, or when it is a faction structure; and a rider of `TransportSlotCount = 0`.

## Inside

- **A rider is held.** `GarrisonContain::onContaining` disables its rider `DISABLED_HELD`, gives it the weapon bonus condition `GARRISONED`, puts it at the container's position when the container encloses it, and lets the structure attack (`OBJECT_STATUS_CAN_ATTACK`); `onRemoving` undoes them. `GameData.ini`: `WeaponBonus = GARRISONED RANGE 133%` and `GARRISONED DAMAGE 125%`.
- **A garrison takes its rider's player.** `GarrisonContain::recalcApparentControllingPlayer` sets the container's team to its first rider's player's default team while it holds a rider, and back to its original team when it is empty; the drawable takes `MODELCONDITION_GARRISONED` while it holds a rider its viewer may see.
- **Fire points.** An enclosing garrison reads its fire points from the bones `FIREPOINT01` on of its model in the `GARRISONED` state, again in `DAMAGED` and in `REALLY_DAMAGED` (`loadGarrisonPoints`); a rider that attacks moves to the free fire point nearest its victim when that point is nearer than its own (`trackTargets`). 4 objects say `IsEnclosingContainer = No` and fire from station points instead.
- **A transport's riders.** `TransportContain::isPassengerAllowedToFire` lets only an infantry rider fire, and only from a container of `PassengersAllowedToFire`; the riders move with the container (`moveObjectsWithMe`).
- **A heal inside.** A container of `HealthRegen%PerSec` heals each rider by that share of its most health each second.

## Out

- **An exit.** A rider leaves by the container's exit paths, `NumberOfExitPaths`, one every `ExitDelay` (`OpenContain::exitObjectViaDoor`); the command set's `Command_TransportExit` buttons, at the Humvee's places 4 to 8, show each rider and let it out, and `Command_Evacuate`, at 9, lets out every rider.
- **A container's death.** `OpenContain::onDie` deals each rider `DamagePercentToUnits` of its most health as `UNRESISTABLE` damage, kills it at 100 %, and lets out the survivors.
- **A body's damage state.** `ActiveBody::calcDamageState` makes a body `REALLYDAMAGED` at a ratio of its health to its most at `UnitReallyDamagedThreshold` or below, and above 0 (read 28 October 2026).
- **A garrison past its threshold.** `GarrisonContain::onBodyDamageStateChange` orders every rider out as the container reaches `REALLYDAMAGED`, unless `GARRISONABLE_UNTIL_DESTROYED`; `GameData.ini` gives `UnitReallyDamagedThreshold = 0.35` and `UnitDamagedThreshold = 0.7`.
- **An initial payload.** `InitialPayload` makes that many riders of the type inside a container as it is made.

## A sale and an assault

Read 28 October 2026.

- **A sold container lets its riders out.** `BuildAssistant::sellObject` calls the container's `onSelling`, and `OpenContain::onSelling` orders every rider out (`orderAllPassengersToExit`), as the sale's scaffold counts down; a rider still inside as the object goes is destroyed with it (`OpenContain::onDelete`). The engine's sale despawns its unit as its time ends (`Sales::end_sales`), and its containers let riders out only as they die (`Containers::fall`, on `Added<Dead>`), so a sold container's riders stay out of the world.
- **An assault transport deploys its riders.** `ChinaVehicleTroopCrawler` runs an `AssaultTransportAIUpdate`, `MembersGetHealedAtLifeRatio = 0.5`: a player's attack or attack-move order makes it let out each rider above that share of its life to fight the target, and call back each below it to heal inside; an idle order calls them all back (`AssaultTransportAIUpdate::aiDoCommand`, `update`). The bot gives no order to let riders out, so a crawler's 8 Red Guards of `InitialPayload` never fight.

## The Chinook

- **It lands for its riders.** `ChinookAIUpdate::update` lands an idle Chinook while a unit waits to enter or exit it, and takes off once none waits; a rider leaves only a landed Chinook, or a combat drop's rappel (`getAiFreeToExit`).

## OpenRA

- **Cargo.** `RideTransport` loads its passenger into the transport's `Cargo` and removes it from the world (`World.Remove`) at the frame's end, and a `Cargo` trait holds its passengers as live actors out of the world, killing them as it dies unless `EjectOnDeath`, by `MaxWeight` against each `Passenger`'s `Weight`; `Types` against the passenger's `CargoType` choose who enters; `UnloadCargo` lets them out at the actor's exits; `InitialUnits` fill it as it is made. `AttackGarrisoned` lets a `Cargo`'s passengers fire from its `FirePorts`, each a point and a cone of the actor's.

## The engine's riding

Read 28 October 2026.

- **Bevy's entity disabling.** A disabled entity stays in the world with its relationships; a query that does not name the disabling component passes over it, and `World`'s methods reach it; Bevy warns that "custom disabling components can cause significant interoperability issues within the ecosystem" ([entity_disabling](https://docs.rs/bevy_ecs/latest/bevy_ecs/entity_disabling/index.html)).
- **bevy_replicon 0.45.2.** The server replicates by walking the world's archetypes, disabled entities among them; a visibility filter's insert observer gets its entity by `Query<(Entity, &F), (Without<ClientVisibility>, Allow<Disabled>)>` and unwraps it (`src/server/visibility.rs:132`), and the observer of a new client's filter names no disabling component at all. The engine's `Rooms` and data filters are such filters.
- **OpenRA.** `RideTransport::OnEnterComplete` loads the passenger into its transport's `Cargo` and calls `World.Remove`, which keeps the actor; `Cargo` keeps its passengers in a list and unloads the last first, kills or ejects them as it dies, crediting its killer, and gives them its new owner as it changes hands ([RideTransport.cs](https://raw.githubusercontent.com/OpenRA/OpenRA/bleed/OpenRA.Mods.Common/Activities/RideTransport.cs), [Cargo.cs](https://raw.githubusercontent.com/OpenRA/OpenRA/bleed/OpenRA.Mods.Common/Traits/Cargo.cs), [World.cs](https://raw.githubusercontent.com/OpenRA/OpenRA/bleed/OpenRA.Game/World.cs)).
