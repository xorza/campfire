# Unit picking

A proposal for one slice. When it ends, each lasting decision moves into the document of the system it governs, and this document is deleted ([A slice's proposal is temporary](../../AGENTS.md#documents)); its plan is in `PLAN.md`. Its facts are in [Unit picking](../research/unit-picking.md).

**At the slice's end** a player points at a unit beside or in front of a building and the unit, not the building, takes the hover and the click, and a small unit is as easy to point at as a large one at any zoom, each as the mode's data chooses.

## Decisions

### The client

- **A unit is picked by its shape, not by its meshes.** The ray through the cursor meets each living unit's `UnitShape`, the upright cylinder of its body's circle or the turned box of its body's box, as tall as its look, which every drawing root already holds. Practice: StarCraft II's selection radius, League of Legends' selection area, OpenRA's selection bounds, each a shape of its own apart from the model. Refused: the box of each mesh, today's pick, whose empty corners over a building's pad take the cursor from the unit beside it; the triangles of each mesh, as Zero Hour casts them, which keep every mesh in the main world and still leave a thin unit hard to hit; an ID buffer, whose pass and readback lag a frame for a scene this small.
- **Picking is a Bevy picking backend.** A system in `PickingSystems::Backend` reads each pointer's ray from `RayMap` and sends `PointerHits` of the units it meets, so the hover, the UI's blocking and the pointer events are Bevy's own for units as for UI nodes, and a UI node, of a higher `order`, hides the unit under it. Refused: a pointer of the client's own beside Bevy's picking, which tests the UI apart.
- **Tiers are pick layers.** The mode's tiers rank the units the ray meets: each tier's hits go in a `PointerHits` of its own, of an `order` between the camera's and the UI's, a higher tier above, and within a tier the nearest along the ray comes first, the lower stable id at one depth, as Bevy orders its picks. Practice: OpenRA's selection priority, Spring's preference for mobile units. Refused: a priority folded into the depth, which breaks the depth's meaning for every reader.
- **A least size on screen.** A shape's radius, and each half edge of a box, widens to the mode's least radius in pixels at the shape's depth, so a unit far off or small stays as easy to point at as StarCraft II's selection radius keeps it. Refused: a widening in meters, which shrinks with the zoom.
- **One ranked list for the client's queries.** The backend keeps the mouse's ranked units of the frame, which the client's pointer reads to skip the units a query leaves out, as an order skips its own selection; the hover takes the first. Refused: each query casting its own ray, which could disagree with the hover within a frame.

### The mode's options

- **The mode chooses its picking.** The mode's `client/picking.toml` gives `tiers`, a list of filters of a unit's relation to the player and its type's tags, which every client holds as the load gives them, a unit in the tier of the first filter that selects it and those no filter selects in a last tier, and `least_radius_px`; a mode of no file has one tier and no least size, the shapes alone. Practice: the mode's `client/hud.toml` and `client/keys.toml`, each a choice of the mode's presentation. Refused: an engine rule that units come before buildings, which a mode of no buildings or of selectable walls would not want.
- **Zero Hour's units come before its structures.** The import writes `tiers = ["all:!structure"]` and a least radius of 10 pixels, as StarCraft II and Spring prefer units, where the original picks by its nearest triangle alone. Refused: the original's pick, which this slice gives up for the player's convenience.
