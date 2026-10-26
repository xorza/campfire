# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

## Ready

- **Client.** **Plan: P2.** A rollback that moves a predicted shot of a slot to a later tick gives the slot a ready tick past the one `FireSeen` holds, so the client takes it for a second shot: it plays the shot's fire effect and muzzle flash again and draws a second stand-in (`MatchEffects::play_fires`, `ShotStandIns::place`).
- **Combat.** **Plan: P3.** A unit whose type despawns at death, as every faction building of the Zero Hour import, is despawned at the end of the tick it died in, so no client receives its `Dead`, and a client plays no death effect for it: a destroyed building vanishes with no explosion ([Zero Hour's deaths](../docs/research/zero-hour-deaths.md#what-the-engine-shows)).
- **Import.** **Plan: E4.** A look's death names one effect, which the import takes from an `FXListDie`'s `DeathFX` alone, so a vehicle whose normal death is a `SlowDeathBehavior`, as the Crusader, Battlemaster, Overlord, Humvee and Tomahawk, plays no effect and vanishes when its keep time ends; its slow death's FX of its three phases are neither converted nor counted as a loss ([Zero Hour's deaths](../docs/research/zero-hour-deaths.md)).
- **Import.** A death's hulk and thrown debris, a slow death's weapons, and a tracked vehicle's tread dust and track marks are not converted, for a slice of wrecks after Effects ([Zero Hour's deaths](../docs/research/zero-hour-deaths.md)).
- **Net.** **Plan: P1.** A bot's first order after a start or a restart leaves while its lead over the server is about 1 tick, where the sync's objective is 2, as the sync's deadband of 1 lets a lead of 1 stay, so a stall of every process, as macOS CI runners make, leaves the order a margin of the phase to the server's next tick alone, and `lan.2` fails when the order misses it (run 40328962123: slot 1's input stamped 20 took effect in tick 21; [LAN late input](../docs/research/lan-late-input.md#the-lead-of-a-first-order)).


