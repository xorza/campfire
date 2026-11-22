# Zero Hour's power

How Zero Hour counts a player's power, what a shortage slows and switches off, what the shipped objects give and take, what Red Alert and OpenRA do, and what the engine holds of it, read on 22 November 2026 from [GeneralsX](https://github.com/fbraz3/GeneralsX) at `0e79905d1` (`Energy.cpp`, `Player.cpp`, `Object.cpp`, `ThingTemplate.cpp`, `ProductionUpdate.cpp`, `DozerAIUpdate.cpp`, `OverchargeBehavior.cpp`, `AISkirmishPlayer.cpp`, `Drawable.cpp`, `W3DControlBar.cpp`), the install's `INIZH.big` (`GameData.ini`, `Object/*.ini`), TheSuperHackers' issues, OpenRA at `b70cab2b4` and Red Alert's released source at `f1f0d42bc`, and the engine's code.

## Supply and demand

- **One signed number per type.** A template's `EnergyProduction` gives power when above 0 and takes its absolute value when below; the player's `Energy` sums the two apart, as production and consumption, each an `Int` (`Energy::objectEnteringInfluence`).
- **An object counts while it is its player's and complete.** It joins its player's sums as it joins a team of the player and leaves as it leaves one, at its death, its removal or a capture, which moves it to its new owner (`Player::becomingTeamMember`); an object under construction counts for nothing until its site completes (`Player::onStructureConstructionComplete`). A sold building keeps counting until its removal: `BuildAssistant::sellObject` changes no power.
- **A disabled producer gives nothing; a disabled consumer still takes.** An object of positive `EnergyProduction` takes its power, its bonus included, out of its player's production as it becomes disabled by any cause and puts it back as it recovers (`Object::onDisabledEdge`); a consumer's share never changes with its state, "or else we go low power which removes the consumption which makes us not low power".
- **A bonus by an upgrade or an overcharge.** A template's `EnergyBonus` joins its player's production while its `PowerPlantUpgrade` is upgraded (America's Control Rods, `Upgrade_AmericaAdvancedControlRods`) and while its `OverchargeBehavior` is on (`Energy::addPowerBonus`). An overcharge is a toggle of the panel; while on, it damages its plant each frame by `HealthPercentToDrainPerSecond` of its maximum life over 30, as `DAMAGE_PENALTY` from itself, and turns off when its life falls below `NotAllowedWhenHealthBelowPercent` of its maximum, 3 % and 0 % in the shipped data (`OverchargeBehavior::update`).
- **Sabotage.** A saboteur that enters a power plant sets its player's `m_powerSabotagedTillFrame` (`SabotagePowerPlantCrateCollide`); until that frame its production reads 0 and its ratio 0.

## The shortage

- **Short when production is below consumption.** `Energy::hasSufficientPower` is `production ≥ consumption`, so a player of no power and no use is not short.
- **Every change re-applies the brownout.** Each change of either sum calls `Player::onPowerBrownOutChange(!hasSufficientPower())`, which disables or enables the player's radar and walks every object of the player, setting `DISABLED_UNDERPOWERED` on each of `KindOf = POWERED` while short and clearing it otherwise; an object that joins later is caught by the walk its own entry starts.
- **The radar goes dark.** `Player::disableRadar` makes `hasRadar` false unless one of its radars is `DisableProof`, the GLA Radar Van's (`DisableProof = Yes ; Won't be disabled by power low`); the command centers' radars are not, though the command centers are not `POWERED`.
- **What a disabled object stops.** An update module runs while its object is disabled only by a type its `getDisabledTypesToProcess` names; `AIUpdate` names only `DISABLED_HELD`, so a disabled object neither moves nor turns its turret nor fires (`GameLogic::update`), and `Object::isMobile` is false. Its special powers' timers pause (`pauseAllSpecialPowers`), a portable structure it carries is disabled with it, a dozer drops its task (`DozerAIUpdate::onDisabledEdge`), and a `ProductionUpdate` produces only under the disabled types its `DisabledTypesToProcess` lists.
- **What it shows.** The drawable fades to a tint of −0.5 on each channel over 30 frames (`DARK_GRAY_DISABLED_COLOR`) and shows the disabled icon over its health bar; its model's animations pause unless its draw module sets `AnimationsRequirePower = No`; a structure plays the building's disabled sound and a vehicle the vehicle's, and the re-enabled sound as the last disabling cause clears (`Object::setDisabledUntil`, `Object::clearDisabled`). EVA's low-power warning plays while the local player is short (`Eva::shouldPlayLowPower`).
- **The power bar.** The control bar draws production as a bar of `log₇(production) × width / 3` pixels and consumption as a needle at `log₇(consumption)`, 1.5 for a consumption of 1; the bar is yellow when consumption is within 5 below production or equal to it, red when it passes production, green otherwise (`W3DPowerDraw`, `PowerBarBase = 7`, `PowerBarIntervals = 3`, `PowerBarYellowRange = 5` by default).

## The production speed

- **A penalty by the ratio.** `ThingTemplate::calcTimeToBuild` takes the frames `BuildTime × 30`, truncated, times the handicaps, and divides them by a rate `p`: with `r` the ratio clamped to at most 1, `p = max(1 − (1 − r) × LowEnergyPenaltyModifier, MinLowEnergyProductionSpeed)`, then, if `r < 1`, `min(p, MaxLowEnergyProductionSpeed)`; the quotient is truncated to an `Int`. `GameData.ini` sets 1.0, 0.5 and 0.8, so a short player builds at 0.5 to 0.8 of full speed, and at 0.8 however little it is short.
- **No use is a ratio of 0 unless something gives.** `Energy::getEnergySupplyRatio` returns production over consumption, and production itself when consumption is 0, so a player of no power and no use has a ratio of 0 and builds at 0.5, though it is not short: a GLA player always, and an American or Chinese one from the start until its first power plant completes. A GLA Rebel of `BuildTime = 5.0` takes 300 frames, 10 s. TheSuperHackers keep it as the game's balance: "Players are penalized for having zero power production, even if the power consumption is also zero", and a GLA player who captures a dozer and builds a power plant halves every build time ([#1390](https://github.com/TheSuperHackers/GeneralsGameCode/issues/1390)).
- **A train's progress is recomputed each frame.** `ProductionUpdate::update` counts the frames its head has been under production and sets its share to those frames over `calcTimeToBuild` at the current ratio, so a change of power changes the share of the time already spent: a train past half its time when the first power plant completes ends at once ([#1390](https://github.com/TheSuperHackers/GeneralsGameCode/issues/1390)).
- **A research is not slowed.** `UpgradeTemplate::calcTimeToBuild` returns `BuildTime × 30` with "@todo modify this by power state of player".
- **A site accumulates.** A dozer or a worker building at its site adds `100 / calcTimeToBuild` percent each frame at the current ratio, in `f32`, and the site gains `maxHealth / calcTimeToBuild` life (`DozerAIUpdate`, whose state machine the worker shares).

## The skirmish AI

- **A short AI builds power first.** `AISkirmishPlayer::processBaseBuilding` takes the first power plant of its build list, of `KindOf = FS_POWER` and not `CASH_GENERATOR`, ahead of its next building while it is short and no power plant of its is under construction, and queues a dozer when it is short with none.

## The shipped numbers

- **Factions.** From `Object/FactionBuilding.ini`; the generals' copies change only the numbers they name.

| Object | `EnergyProduction` | `EnergyBonus` | `POWERED` |
| --- | --- | --- | --- |
| `AmericaPowerPlant` | 5 | 5 (Control Rods) | no |
| `ChinaPowerPlant` | 10 | 5 (overcharge) | no |
| `AmericaCommandCenter`, `ChinaCommandCenter`, every GLA object | 0 | — | no |
| `AmericaWarFactory`, `AmericaAirfield`, `AmericaSupplyCenter`, `ChinaWarFactory`, `ChinaAirfield`, `ChinaSupplyCenter` | −1 | — | no |
| `ChinaSpeakerTower` | −1 | — | yes |
| `AmericaStrategyCenter`, `ChinaPropagandaCenter` | −2 | — | no |
| `AmericaPatriotBattery`, `ChinaGattlingCannon` | −3 | — | yes |
| `AmericaSupplyDropZone` | −4 | — | no |
| `AmericaParticleCannonUplink`, `ChinaNuclearMissileLauncher` | −10 | — | yes |

- **The generals.** Power plants: the Laser General's 8 with a bonus of 8, the Superweapon General's 5 with 15, the Nuke General's 18 with 9; the others as their faction's. The Laser General's Patriot takes 5, his Laser Cannon and the Superweapon General's Cruise Missile Launcher and Nuclear Missile Launcher take 10 and are `POWERED`; his Crusader takes 1 and is `POWERED`, his Paladin takes 1 and is not, the only units that take power. `GameData.ini` sets `MinLowEnergyProductionSpeed = 0.5`, `MaxLowEnergyProductionSpeed = 0.8`, `LowEnergyPenaltyModifier = 1.0`.

## Other engines

- **OpenRA's power manager.** Each actor's `Power` traits give their `Amount`, positive or negative, times its power modifiers, as `ScalePowerWithHealth` scales it by its health share, to its owner's `PowerManager`, which keeps provided and drained as each actor is added, changed, removed or changes owner. Its state is `Normal` when provided ≥ drained, `Low` when provided passes half the drain and `Critical` below, so no power and no drain is `Normal` ([PowerManager.cs](https://github.com/OpenRA/OpenRA/blob/bleed/OpenRA.Mods.Common/Traits/Power/Player/PowerManager.cs)).
- **OpenRA's effects are conditions.** `GrantConditionOnPowerState` grants a condition at the states it names, and the mod's `^DisableOnLowPower` grants `disabled` from it, which every trait that requires its absence stops on and an overlay of black at 70 % shows; production multiplies its remaining time by the queue's `LowPowerModifier` percent while the state is not `Normal`, by skipping ticks ([GrantConditionOnPowerState.cs](https://github.com/OpenRA/OpenRA/blob/bleed/OpenRA.Mods.Common/Traits/Conditions/GrantConditionOnPowerState.cs), [ProductionQueue.cs](https://github.com/OpenRA/OpenRA/blob/bleed/OpenRA.Mods.Common/Traits/Player/ProductionQueue.cs), [defaults.yaml](https://github.com/OpenRA/OpenRA/blob/bleed/mods/ra/rules/defaults.yaml)). A forum report says `RequiresPower` on a unit stops its fire but not its move ([forum](https://forum.openra.net/viewtopic.php?p=283532)).
- **Red Alert's power fraction.** `HouseClass::Power_Fraction` is 1 when power meets the drain or nothing drains, else power over drain; `TechnoTypeClass::Time_To_Build` multiplies the time by 4 at a fraction of 0, by 2.5 below one half and by 1.5 below 1. Below 1, buildings that drain power and stand above the yellow health share take 1 damage each `DamageDelay` minutes, and superweapon charges are suspended (`HOUSE.CPP`, `TECHNO.CPP`, [CnC_Remastered_Collection](https://github.com/electronicarts/CnC_Remastered_Collection)).

## What the engine holds

- **No power.** No unit type section, player count or import of `EnergyProduction`, `EnergyBonus` or `KindOf = POWERED`; the import writes each build's and train's time as `BuildTime × 30` frames with no penalty (`game_time/mod.rs:39`), so every build and train of the import runs at the original's full speed, a GLA player's twice as fast as the original's.
- **A train's time is fixed at its head.** A queue's head ends at the tick its entry's time from the tick it reached the head gives (`train_queue.rs:102`), which the index of the queues holds as its due tick.
- **A site grows by a rate.** A site's progress adds its build's rate for its count of builders each tick toward its time, its life the gain of that progress, exactly (`site.rs:125`, `construction/mod.rs:315`).
- **Supply is a derived count.** The holdings keep what each unit counts for and follow the marked units (`holdings.rs:31`); each player's supply is written on its entity as each tick ends and sent on change (`player_supply.rs:20`).
- **Effects by tags and player modifiers.** A tag may block moving and each action group (`tag_properties.rs:51`; the rules package's `second_life` blocks `move`); a player modifier holds on each of its units its `affects` selects, by `Hold::Player` (`lifetime.rs:35`), and a type's `upgrades` hold a modifier while its player's or its own modifiers meet their triggers.
- **The Control Rods' tag.** A `PowerPlantUpgrade` converts to the tag `power_plant_upgraded`, which its states test, and no power (`upgrade_import/mod.rs:244`).
- **The minimap.** It shows while the player holds a unit of the HUD file's radar tag, which a `RadarUpgrade` gives (02-engine-core.md, A minimap by a radar tag).
- **The bot.** It builds the first base building of its list that it lacks, one of each (`bot.rhai:87`).
