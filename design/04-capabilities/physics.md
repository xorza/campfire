# Physics

## Mechanism

Vehicles, rigid bodies and terrain: a collision backend that moves bodies by forces.

## Data

A unit type's `physics` section: its shape, mass and, for a vehicle, its wheels and engine. The map's heightmap terrain.

## Rules

- **Backend:** a physics engine such as Rapier in its cross-platform deterministic mode. The only place floating point is allowed in the sim; the goldens check it on every OS ([Collision](../02-engine-core.md#backends)).
- **Terrain:** heightmap collision for maps around 8 × 8 km.
- Ragdolls and debris are client-side presentation.

## State and derived

- **State:** each body's position, rotation and velocities, as the backend keeps them, in the snapshot.

## Network

Vehicles a client drives are predicted; other bodies are interpolated.

## Cost

The backend's step, which grows with the bodies in contact.

## Genres

A battle royale's vehicles; any game with rigid bodies.
