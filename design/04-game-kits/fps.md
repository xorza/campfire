# FPS kit

Status: later.

## Inputs

One input frame per tick: movement keys, view angles (yaw, pitch), buttons (fire, jump, crouch, use, reload), and the tick the player's screen was showing. Typical tick rates: 64–128 Hz.

## Movement

Character controller: a 3D capsule against level geometry, with gravity, jumping, stairs, slopes, crouching and ladders. Clients predict their own movement every frame with the same code.

## Weapons and hits

- Weapon data: fire rate, damage, falloff, penetration, recoil, spread.
- Spread comes from the sim RNG on the server only. The client predicts the shot effect, not where it lands: a client that knows the spread can cancel it.
- Hitscan: ray tests against per-body-part hitboxes. Hitboxes follow a simplified animation pose computed in the sim.

## Lag compensation

The sim keeps a short history of hitbox poses as part of its state and tests each shot at the tick named in the shooter's input. That tick is in the recorded input and the history is sim state, so the verifier gets the same hit. It is kit code, not the network layer's lag compensation, because the verifier runs no network layer.

## Visibility

3D occlusion backend: an enemy is sent only when visible or about to become visible. Footsteps and gunshots go only to players within hearing range.

## Bots

Navmesh pathfinding backend.

## Mode scripts

Rounds, buy time, bomb, economy and team swaps are mode scripts, built on core primitives (timers, freeze, respawn, team changes).
