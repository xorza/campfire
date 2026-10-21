# Zero Hour continuous fire

How a Zero Hour weapon fires faster as it keeps firing, read on 21 October 2026 from GeneralsX's `GeneralsMD` source at `0e79905d1` and the shipped INI of `INIZH.big`; and how OpenRA and Ares do it, for comparison. Paths are under `GeneralsMD/Code/GameEngine/`.

## Zero Hour

- **The count.** Each shot of an object adds one to its count when it hits the same victim, or another victim before the coast ends; else the count starts again at 1 (`FiringTracker::shotFired`). The coast ends `ContinuousFireCoast` after the frame the weapon may next fire.
- **The stages.** With no stage, a count above `ContinuousFireOne` sets `CONTINUOUS_FIRE_MEAN`; with it, a count above `ContinuousFireTwo` sets `CONTINUOUS_FIRE_FAST`, and a count below `ContinuousFireOne` clears it; with `FAST`, a count below `ContinuousFireTwo` clears it. The two are exclusive, and a clear goes to no stage (`FiringTracker::speedUp`, `coolDown`).
- **The end.** Past the coast with no shot, the stage clears (`FiringTracker::update`).
- **The state is the object's, the bonus the weapon's.** The stage is a weapon bonus condition of the object; each weapon's `WeaponBonus` lines give what the condition is worth to it.
- **The rate.** The delay is the weapon's delay in frames divided by the summed `RATE_OF_FIRE`, rounded down (`WeaponTemplate::getDelayBetweenShots`, `REAL_TO_INT_FLOOR`), computed as the shot fires, before `shotFired` counts it.
- **Bonuses add.** Each condition's bonus adds its value less 1 to each field (`WeaponBonus::appendBonuses`), so 200 % and 150 % make 250 %.
- **The weapons.** `GattlingBuildingGun`: 250 ms, stages above 1 and 5 shots, 200 % and 300 %, coast 2,000 ms, so 8, 4 and 2 frames. `GattlingTankGun`: 400 ms, 2 and 6, 200 % and 300 %, 1,000 ms. `Infa_MiniGunnerGun`: 500 ms, 6 and 12, 200 % and 300 %, 1,000 ms. `SpectreHowitzerGun`: 777 ms, 1 and 2, 150 % and 200 %, 2,000 ms.
- **Other weapon bonuses.** The INI's `WeaponBonus` lines name 20 condition and field pairs: `PLAYER_UPGRADE DAMAGE` 86 times, the two continuous-fire stages 8 times each, and veterancy, horde, nationalism, frenzy, garrison and drone spotting once or twice.

## Other engines

- **OpenRA.** `GrantConditionOnAttack` grants an instance of a condition after `RequiredShotsPerInstance` shots, up to `MaximumInstances`, and revokes one, or all with `RevokeAll`, after `RevokeDelay` ticks with no shot, or on a new target with `RevokeOnNewTarget`; a `ReloadDelayMultiplier` enabled by the condition shortens the reload ([source](https://raw.githubusercontent.com/OpenRA/OpenRA/bleed/OpenRA.Mods.Common/Traits/Conditions/GrantConditionOnAttack.cs)).
- **Ares.** A gattling unit holds a charge that rises by `RateUp` each frame it fires and falls by `RateDown` each frame after it loses its target; the charge picks one of `WeaponStages` weapons ([Ares](https://ares-docs.readthedocs.io/zh-cn/latest/new/gattlingcycle.html), [CnCNZ](https://forums.cncnz.com/topic/306-gattling-weapons-guide/)).
