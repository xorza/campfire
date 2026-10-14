# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## 1. The import's objects and materials

`import`: `zero_hour`; `package`: `client_data` and its tests; `checks/zero-hour-check`. What the import writes, and the generic formats it writes, before the client reads them.

**Slice 3, Zero Hour's map as the original draws it, still,** batches 1 and 2. The user opens Tournament Desert of their imported Zero Hour in the client, and sees each object as the original draws it at the start of a multiplayer game, as near as the engine's one material holds it: each model at its asset scale, rough and of no specular, unlit where the game lights it not, with its second passes and detail stages as layers, and the game's `MissingTexture`; its props, optimized trees and fluff drawn by the client alone, no unit of the sim; no object of no INI template. The engine stays game-agnostic: every rule of W3D is the importer's, which converts to Bevy's standard material, glTF and the generic client files, and writes down what it loses (design 12, D13). It takes the two foundations of the import, and every Zero Hour item of the issue log. The design is [Zero Hour](docs/design/12-zero-hour.md), D13 and D14. Later slices of the look: roads and bridges, which the original's road buffer draws from 82,397 map objects; water; each condition state's model and animation, the night's and the snow's among them; a player's house color; the trees' sway and topple and the props a new structure removes, which read units that move and build; shadows. The slice ends when the user compares Tournament Desert in the client with the original at the same camera.

### Z1. Client objects and the asset scale

(`import`, `package`, the Zero Hour check; D14; **Zero Hour**, "The import makes a unit of every object", "The import draws each model at scale 1".) The object INI reader reads each object's `KindOf`, `FenceWidth` and `Scale`, a reskin keeping its base's `KindOf` and giving its own `FenceWidth` and `Scale`. The map import drops an object of no INI template, writes an optimized tree, a prop and fluff to `client/maps/<name>/objects.toml`, and places every other as now; `data/units.toml` holds the templates of placed units alone. `package` gains the file's type, `ClientObjects`, and a model's `scale` in `client/units.toml`. Check: an import of hand-written INI and map fixtures puts each kind where D14 says, a reskin of a prop among the props, a fence that a build clears among the units, a reskin's scale its own; the Zero Hour check pins Tournament Desert's units, unit types and client objects, and logs each kind's templates for review.

### Z2. The material's conversion

(`import`; D13; **Zero Hour**, "The import lights every material", "A texture no archive holds".) Every material rough and of no specular, `KHR_materials_specular` at a factor of 0; unlit, `KHR_materials_unlit`, for a shader whose primary gradient is off; a missing texture's triangles of `0x7FFF00FF` times the material's color. Check: the fixtures' glTF materials, each field computed by hand, and their extensions declared in `extensionsUsed`.

### Z3. Layers

(`import`, `package`; D13; **Zero Hour**, "The import draws a mesh's first material pass".) The W3D mesh reader keeps each pass and each stage; the model import writes a second pass and a detail stage each as primitives of the same vertices, with their own coordinates, vertex colors and material, a stage with no coordinates at 0, 0 as the first's; the detail function's scale a multiplying blend and its add an additive one, a detail alpha function lost, as D13 writes. `MaterialFile` gains `depth_bias`, which each layer takes above the one under it. Check: a fixture mesh of two passes, one additive and one by alpha with its own vertex alphas, and one of a detail stage of each function, writes each primitive over one vertex list, each layer's coordinates, colors, blend and depth bias computed by hand, the first pass unchanged.

### P1. The flaw test within its bound

(`package`'s tests; **Package**, "`mode_package::every_flaw_of_a_package_fails_its_load`".) The 273 flaws of the 3v3, each a whole load, run as several tests by what they break, each within 1 s, in place of one of 1.05 s.

## 2. The client draws them

`client`: `view`. Once the import writes them, the client reads them, as it reads any package's.

### Z4. The client objects drawn, at their scale

(`client`; D14.) The client reads each map's `objects.toml`, and spawns each entry as a still model of its look at its ground point and height on the heightmap, turned by its angle, with the nodes its look hides hidden; none is a unit. Every model, a unit's or a client object's, takes its `scale`. Check: what the client spawns of a fixture file, each place, turn and scale computed by hand.

### Z5. Layers drawn

(`client`; D13.) A material file's `depth_bias` is its material's, so a layer draws over the primitive under it at its depth, after it. Check: the material a file of a depth bias gives, and the order of two blended layers of one mesh, computed by hand; a model the import writes of a fixture loads in Bevy rough, of no reflectance, and unlit where its shader turns the lighting off; the user compares Tournament Desert in the client with the original at the same camera: its trees and props, its scales, its unlit glows, its second passes and detail stages.

## Foundations

- **The import's materials are glTF's** (Zero Hour). The import writes only what glTF's material holds, one pass, no ambient color apart from the diffuse one and no fallback texture, so the presentation loses each, and each later fidelity gap of a W3D material meets the same limit; design 12's D13 converts each to the engine's one material and its layers, or writes it down as a loss. Items: Zero Hour, "The import lights every material"; Zero Hour, "A texture no archive holds"; Zero Hour, "The import draws a mesh's first material pass".
- **The import has no object that only the client shows** (Zero Hour). Every object a map places becomes a unit of the sim, where the game spawns props, optimized trees and fluff on the client alone and some objects not at all, so the sim, its log and its state hash hold objects the original never simulates, and the parity oracle meets them; design 12's D14 gives them a client file of the map, which no code writes or reads yet. Items: Zero Hour, "The import makes a unit of every object a map places".
