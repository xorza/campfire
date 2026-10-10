# Zero Hour

Design: [Zero Hour](../design/12-zero-hour.md), the rules package in `source/packages/zero-hour/`. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

- A texture no archive holds leaves its triangles untextured in the import, where the game draws `MissingTexture`, a solid color of `0x7FFF00FF`: draw them as the game does, or untextured, as now.
- The import ignores a vertex material's ambient color, as glTF's material has none: 336 vertex materials of converted models have one other than their diffuse color, which the game lights by the scene's ambient light. Options: a material file with the ambient color, which the client draws; or the diffuse color for both, as now.

## Research

- The import draws a mesh's first material pass and its first texture stage alone; 275 meshes of converted models have a second pass, which the game draws over the first.

- The import makes a unit of every object a map places but a road's or bridge's end and a waypoint, whatever its template: the game spawns no object whose template no INI object names, adds props, optimized trees and, in multiplayer, fluff to the client alone, and spawns no shrub when trees are off (`GameLogic::startNewGame`).

## Ready

- The import turns a waypoint's angle into degrees, though no marker keeps it, and so refuses a map whose waypoint has an angle that the game's `normalizeAngle` never ends on, as an infinite one; the game never normalizes a waypoint's angle (`MapImport::new`, `MapObject::placement`).
