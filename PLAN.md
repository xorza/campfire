# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Slice: Unit picking

A player points at a unit beside or in front of a building and the unit takes the hover and the click, and a small unit is as easy to point at as a large one, as the mode's data chooses ([proposal](docs/design/12-unit-picking.md)). Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. The units' picking backend (L)

`client`, 02-engine-core.md's Client. First: the mechanism, at its defaults, which batch 2's options configure.

- **A pick shape's hit.** A function of a `UnitShape`, its drawing root's transform, a ray and a least radius in world meters gives the depth at which the ray enters the shape: the upright cylinder of a circle footing, the turned box of a box footing, from the ground to the shape's height, each widened to the least radius, none for a ray that misses or enters behind its origin. Builds: one home for each operation.
- **The backend.** A system in `PickingSystems::Backend`, after Bevy's ray map, reads each pointer's ray of the match's camera, tests each living unit's shape, the least radius turned into meters at the shape's center's depth by the camera's projection, and ranks the hits by tier, then depth, then stable id; it sends a `PointerHits` for each tier, of `order` above the camera's and below the UI's camera order plus 0.5, and keeps the mouse's ranked units in a resource of the frame, which `Pointer::unit_under` and `own_unit_under` read; each drawing root is `Pickable`, blocking the units below. The meshes' box test, `pointer/unit_meshes`, goes. Builds: Bevy's own mechanisms; a pass visits what changed.
- **The defaults.** Until batch 2 gives the mode's file, one tier and a least radius of 0.
- **Check:** tests of the hit by hand-computed depths: a ray at a cylinder's side and over its top, at a turned box's face and past its corner, from inside, behind, and a widened radius that a ray misses at 0 and meets at 10 px; of the ranking, a farther unit of a higher tier over a nearer one, the lower stable id at one depth, and a skipped unit; of a headless app where `HoverMap` holds the top unit, and a UI node over it hides it; 02's decision "A click picks by the drawn volume" gives way to the proposal's. The client's frame cost is the shapes' tests alone, measured in the bench case `integration/picking/units`, 1,000 units, once in the gate.

### 2. The mode's picking options (M)

`client-files`, `client`, `import`, the test packages, 02, 09. After batch 1, whose defaults it replaces with the mode's choice; last: it ends the slice.

- **The file.** `client/picking.toml`, a `ClientFile` of TOML: `tiers`, a list of filters, and `least_radius_px`, a number at least 0, each its default when absent; the client resolves the filters once as it loads the mode, against the match's tags, refusing a tag the match lacks, and ranks a unit by its type's tags and its team's relation to the player.
- **Zero Hour's choice.** The import writes `tiers = ["all:!structure"]` and `least_radius_px = 10`; the MOBA test packages write `tiers = ["enemies:avatar", "enemies"]`, as a hero's player points at heroes first.
- **The fold.** The decisions go to 02's Client and 09, the facts stay in research/unit-picking.md, and the proposal is deleted.
- **Check:** tests of the file's round trip and its defaults, of a refused tag, of the tiers a file gives, and of the import's file; `cargo drive run zero-hour`, on request, moves its fingerprints; the user plays Tournament Desert in the client and points at a dozer beside a barracks and at infantry far off.

## Foundations
