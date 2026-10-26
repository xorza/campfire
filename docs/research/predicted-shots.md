# Predicted shots

When a player's own unit fires a projectile, what its client draws and when, read from the code on 26 October 2026 and measured from the LAN check's logs of the same day.

## What the client draws

- **The shot is predicted.** A client predicts its own units' slot times (`SlotTimes`, sent predicted) and clip shots, so it takes a rise of a slot's ready tick as a shot at the tick its predicted timeline runs it: it plays the shot's fire effect then (`MatchEffects::play_fires`), and hides the rounds its clip lacks then, a clip of one shot's round until its reload ends (`SlotAttack::lack`).
- **The projectile is the server's.** The server spawns the projectile at the tick it runs the shot, and sends its `Projectile` once; the client gives it a drawing as it receives it (`ViewPlugin::draw_shots`), and its type's models show once their scene spawns, a frame or more after, the first of a type after its file loads.
- **A Tomahawk shows the gap.** A Tomahawk's clip of one shot hides its missile on the rail at the predicted shot (09-zero-hour.md, A clip of one shot is a clip), and its flying missile shows only once its projectile is received and its model spawned; another player's Tomahawk, which its client receives, hides the round and draws the projectile from the server's ticks alike. A player's report of 26 October 2026, of a local match, saw the missile leave the rail a moment before it flew.

## How far the predicted shot leads

- **The lead of a bot's sends.** In the 52 LAN check runs of 26 October 2026, 30 on the Linux workstation and 22 on an M5 Mac, each first order of a bot after a start or a restart, 265 orders, was sent while the bot's predicted timeline led the server's by 0.09 to 7.98 ticks: 0.64 at the tenth percentile, 1.00 at the median, 1.97 at the ninetieth; a bot's lead settles at about 1 tick or about 2, and stays.
- **So the gap.** A predicted shot leads the server's shot by that lead, 33 ms at the median at 30 ticks a second and 66 ms at a lead of 2, before the projectile leaves the server; on a network the link's one way more, and then a client frame and the missile model's spawn. The gap was not timed in a drawn client.

## Other engines

Read on 26 October 2026.

- **Source.** Valve's guide to a weapon entity predicts the fire of a weapon, its view model's animation, its ammo's depletion and its reload, and not the spawn of a projectile: rockets and grenades materialising out of nothing on observers' computers is hard to pass off ([Authoring a weapon entity](https://developer.valvesoftware.com/wiki/Authoring_a_weapon_entity)); Team Fortress 2 lag-compensates no rocket ([Lag compensation](https://wiki.teamfortress.com/wiki/Lag_compensation)).
- **Netcode for Entities.** A client spawns a predicted ghost at its input, which the server's ghost replaces when they match by type and spawn tick, within five ticks; an unmatched one is deleted after a grace period ([Ghost spawning](https://docs.unity3d.com/Packages/com.unity.netcode@1.3/manual/ghost-spawning.html)). A predicted spawn is not rolled back until its first snapshot, so it can mispredict against other predicted ghosts until then ([Prediction edge cases](https://docs.unity3d.com/Packages/com.unity.netcode@1.3/manual/prediction-details.html)).
- **Photon Fusion.** Fusion 1 predicted a spawn by a placeholder the state authority confirmed or failed; Fusion 2 dropped spawn prediction, and its projectile samples either spawn on the state authority and pre-spawn objects ahead to shorten the delay, or keep the shots as data on the firing player, a count or a buffer of fire ticks, positions and directions, which the shooter's own prediction holds and from which each client draws its projectiles ([Projectiles Essentials](https://doc.photonengine.com/fusion/current/technical-samples/projectiles-essentials)).
- **Overwatch.** It predicts abilities and projectiles by default, correcting a misprediction by a rollback to the server's state and a replay ([Overwatch's netcode](https://edgegap.com/blog/game-backend-deep-dive-overwatch-2016-netcode-architecture-rollback)).
- **Practice of shooters on Unreal and Unity.** A client spawns a cosmetic projectile at its shot, keyed so the server's projectile adopts it or replaces it, hides the server's copy on the shooter until then, and lets the server alone decide hits ([bugnet](https://bugnet.io/blog/how-to-fix-duplicate-projectiles-from-predicted-and-server-spawn), [Unity forum](https://discussions.unity.com/t/netcode-client-side-prediction-questions/773329)).

## The engine's own

- **Pre-spawns exist and no sim code uses them.** `net-replication`'s `PreSpawned` matches a client's entity to the server's by a hash of its archetype and its simulated tick, or a given one, and `net-prediction` despawns a pre-spawn a rollback before its tick makes again and expires an unmatched one when the server's applied state reaches its tick (11-net-stack/replication.md).
- **A delivery is the server's.** `DeliveryEntity` is a kind its owner does not predict, and `Projectiles` installs its launches and flights on the server alone, as "a predicting client predicts no delivery".
- **A tracking projectile meets a target it does not predict.** A client holds another player's units as the server last sent them, behind the tick it predicts its own at; a steered or homing projectile it predicted would steer toward its target's place of that older tick, where the server's steers toward its place at the same tick, so its flight would differ from the server's and be corrected.

## Effects in a replay of predicted ticks

Read on 27 October 2026.

- **Netcode for Entities.** `IsFirstTimeFullyPredictingTick` is set in the prediction loop only on a tick predicted the first time; its documentation gives it to effects that cannot be rolled back, a spawn, a particle system or a sound, so that each plays once and a replay does not repeat it ([NetworkTimeFlags](https://docs.unity3d.com/Packages/com.unity.netcode@1.1/api/Unity.NetCode.NetworkTimeFlags.html), [Ghost spawning](https://docs.unity3d.com/Packages/com.unity.netcode@1.3/manual/ghost-spawning.html)).
- **Photon Fusion.** `IsResimulation` is true on a tick the client simulated before, and `IsForward` on one it did not ([Simulation](https://doc-api.photonengine.com/en/Fusion/v1/class_fusion_1_1_simulation.html)).
- **The engine's own.** A rollback replays its ticks through `FixedMain` in `PreUpdate` (`RollbackPlugin::run_rollback`), so the frame's `Update` sees only the state after the replay; `MatchEffects::play_fires` takes a shot from a rise of a slot's ready tick past the latest it saw, over the frame, and does not know whether a tick was replayed.
