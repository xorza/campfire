# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## 1. The client draws them

`client`: `view`. Once the import writes them, the client reads them, as it reads any package's.

**Slice 3, Zero Hour's map as the original draws it, still,** batch 1, after the import that writes what it draws. The user opens Tournament Desert of their imported Zero Hour in the client, and sees each object as the original draws it at the start of a multiplayer game, as near as the engine's one material holds it: each model at its asset scale, rough and of no specular, unlit where the game lights it not, with its second passes and detail stages as layers, and the game's `MissingTexture`; its props, optimized trees and fluff drawn by the client alone, no unit of the sim; no object of no INI template. The engine stays game-agnostic: every rule of W3D is the importer's, which converts to Bevy's standard material, glTF and the generic client files, and writes down what it loses (design 12, D13). It takes the two foundations of the import, and every Zero Hour item of the issue log. The design is [Zero Hour](docs/design/12-zero-hour.md), D13 and D14. Later slices of the look: roads and bridges, which the original's road buffer draws from 82,397 map objects; water; each condition state's model and animation, the night's and the snow's among them; a player's house color; the trees' sway and topple and the props a new structure removes, which read units that move and build; shadows. The slice ends when the user compares Tournament Desert in the client with the original at the same camera.

### Z4. The client objects drawn, at their scale

(`client`; D14.) The client reads each map's `objects.toml`, and spawns each entry as a still model of its look at its ground point and height on the heightmap, turned by its angle, with the nodes its look hides hidden; none is a unit. Every model, a unit's or a client object's, takes its `scale`. Check: what the client spawns of a fixture file, each place, turn and scale computed by hand.

### Z5. Layers drawn

(`client`; D13.) A material file's metallic, roughness, specular and unlit fields are its material's, a specular factor of `f` a reflectance of `f` / 2 as Bevy's glTF loader reads `KHR_materials_specular`, where the client now draws every file material metallic 0, roughness 1 and of Bevy's default reflectance, 0.5, whatever its file says. A material file's `depth_bias` is its material's, so a layer draws over the primitive under it at its depth, after it. Check: the material a file of each field gives, and that of a depth bias, and the order of two blended layers of one mesh, computed by hand; a model the import writes of a fixture loads in Bevy rough, of no reflectance, and unlit where its shader turns the lighting off; the user compares Tournament Desert in the client with the original at the same camera: its trees and props, its scales, its unlit glows, its second passes and detail stages.

## Foundations

- **The terrain is Zero Hour's** (Client). The client's terrain is a format and a lighting rule of one game, drawn by game-bound code of the client, where every game is drawn by one rendering pipeline; each later terrain feature of the presentation, and a second imported game's terrain, meets the same limit. Items: Client, "The client draws an imported map's terrain by a module of Zero Hour's".

