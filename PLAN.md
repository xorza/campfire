# Plan

The open steps, in the order they are done, and the foundations. No step is open. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Foundations

- **The import's materials are glTF's** (Zero Hour). The import writes only what glTF's material holds, one pass, no ambient color apart from the diffuse one and no fallback texture, so the presentation loses each, and each later fidelity gap of a W3D material meets the same limit; design 12's D13 puts each in the material file, which holds no ambient color yet. Items: Zero Hour, "The import writes no material file for a vertex material whose ambient color"; Zero Hour, "A texture no archive holds"; Zero Hour, "The import draws a mesh's first material pass".
- **The import has no object that only the client shows** (Zero Hour). Every object a map places becomes a unit of the sim, where the game spawns props, optimized trees and fluff on the client alone and some objects not at all, so the sim, its log and its state hash hold objects the original never simulates, and the parity oracle meets them; design 12's D14 gives them a client file of the map, which no code writes or reads yet. Items: Zero Hour, "The import makes a unit of every object a map places".
