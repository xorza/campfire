# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## 1. The client draws them

`client`: `view`. Once the import writes them, the client reads them, as it reads any package's.

**Slice 3, Zero Hour's map as the original draws it, still,** batch 1, after the import that writes what it draws. The user opens Tournament Desert of their imported Zero Hour in the client, and sees each object as the original draws it at the start of a multiplayer game, as near as the engine's one material holds it: each model at its asset scale, rough and of no specular, unlit where the game lights it not, with its second passes and detail stages as layers, and the game's `MissingTexture`; its props, optimized trees and fluff drawn by the client alone, no unit of the sim; no object of no INI template. The engine stays game-agnostic: every rule of W3D is the importer's, which converts to Bevy's standard material, glTF and the generic client files, and writes down what it loses (design 12, D13). It takes the two foundations of the import, and every Zero Hour item of the issue log. The design is [Zero Hour](docs/design/12-zero-hour.md), D13 and D14. Later slices of the look: roads and bridges, which the original's road buffer draws from 82,397 map objects; water; each condition state's model and animation, the night's and the snow's among them; a player's house color; the trees' sway and topple and the props a new structure removes, which read units that move and build; shadows. The slice ends when the user compares Tournament Desert in the client with the original at the same camera.

### Z5. The comparison with the original

(`client`; D13, D14.) The client draws the map's client objects, each model at its scale, and each material file by its own fields and depth bias, as the tests check. Check: the user compares Tournament Desert in the client with the original at the same camera: its trees and props, its scales, its unlit glows, its second passes and detail stages; what differs goes to the issue log.

## Foundations

- **The terrain is Zero Hour's** (Client). The client's terrain is a format and a lighting rule of one game, drawn by game-bound code of the client, where every game is drawn by one rendering pipeline; each later terrain feature of the presentation, and a second imported game's terrain, meets the same limit. Items: Client, "The client draws an imported map's terrain by a module of Zero Hour's".

