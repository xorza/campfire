# Battle royale kit

Status: later. Builds on the FPS kit; around 100 players per match.

## Vehicles

A physics backend (e.g. Rapier in cross-platform deterministic mode). The only place floating point is allowed in the sim; `det-ci` checks it on every OS. Ragdolls and debris are client-side presentation.

## Ballistics

Bullets travel and drop. The server rewinds to the firing tick, then simulates the bullet forward.

## Map

Heightmap terrain collision for maps around 8×8 km. Far-field occlusion uses terrain height and distance; full occlusion tests only up close.

## Relevance

What a client receives weighs distance, view direction and scope state; far players update less often (Lightyear bandwidth priority).

## Loot

Items spawn from the seeded RNG, stay dormant until someone is near, and are sent only to nearby clients.

## Mode scripts

Plane drop, shrinking zone, squads and last team standing are mode scripts. Wagers and entry fees work as in any match.

## Main risk

Performance: 100 players at 30–60 Hz with vehicles, bullets and loot in one deterministic process. Requires deterministic multithreading and early `det-ci` benchmarks.
