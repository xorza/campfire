# Physics

Vehicles, rigid bodies and terrain.

- **Backend:** a physics engine such as Rapier in its cross-platform deterministic mode. The only place floating point is allowed in the sim; `det-ci` checks it on every OS ([Collision](../02-engine-core.md#collision)).
- **Terrain:** heightmap collision for maps around 8×8 km.
- Ragdolls and debris are client-side presentation.
