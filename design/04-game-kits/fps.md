# FPS kit

Status: later.

## Inputs

One input frame per tick: movement keys, view angles (yaw, pitch), buttons (fire, jump, crouch, use, reload), and the tick the player's screen was showing. Typical tick rates: 64–128 Hz.

## Movement

Character controller: a 3D capsule against level geometry, with gravity, jumping, stairs, slopes, crouching and ladders. Clients predict their own movement every frame with the same code.

## Weapons and hits

- Weapon data: fire rate, damage, falloff, penetration, recoil, spread.
- Spread comes from the sim RNG, seeded per shot, so client and server agree.
- Hitscan: ray tests against per-body-part hitboxes. Hitboxes follow a simplified animation pose computed in the sim.

## Lag compensation

The server keeps a short history of hitbox poses and tests each shot at the tick named in the shooter's input. That tick is recorded, so replays stay deterministic. Built on Lightyear's lag compensation.

## Visibility

3D occlusion backend: an enemy is sent only when visible or about to become visible. Footsteps and gunshots go only to players within hearing range.

## Bots

Navmesh pathfinding backend.

## Mode scripts

Rounds, buy time, bomb, economy and team swaps are mode scripts, built on core primitives (timers, freeze, respawn, team changes).
