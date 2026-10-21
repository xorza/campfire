# Unit picking

How games and engines pick the unit under the cursor, read on 21 October 2026: Zero Hour from GeneralsX's `GeneralsMD` and `Core` source at `0e79905d1`, the others from their sources and docs as linked.

## Zero Hour

- **A ray against what it draws.** `W3DView::pickDrawable` casts the ray through the cursor into the scene; `RTS3DScene::castRay` tests each visible render object's bounding sphere, then `Cast_Ray` against its geometry, and keeps the nearest hit, shortening the ray at each.
- **Every opaque mesh and every collision box.** `MeshClass::Cast_Ray` tests the mesh's own triangles, skipping a translucent or hidden mesh, as a headlight beam; `Set_Collision_Type` always adds `COLL_TYPE_ALL`, which the inner test asks for, so every mesh and collision box collides whatever its W3D flags (`rendobj.h`, `mesh.cpp`, `boxrobj.cpp`). The install's HLODs hold 680 collision boxes ([Zero Hour research](zero-hour.md), HLOD sub-objects).
- **A translucent mesh lets the ray through.** `MeshClass::Cast_Ray` passes a mesh whose pass-0 shader blends, of a source factor other than one or a destination factor other than zero, with no alpha test (`Is_Alpha`), "so we could select trees but filter out headlight beams"; an alpha-tested mesh, as leaves, is met.
- **No priority.** The nearest hit wins, a building's wall over a unit behind it.
- **A bar stands over the geometry.** `Object::getHealthBoxPosition` raises a unit's bar its geometry's `getMaxHeightAbovePosition` plus 10 over its position, not over its model; the America command center's geometry is 49 high, its building model 52 and its radar tower model 89 (read 21 October 2026 from the imported package).

## Zero Hour's models, imported

- **Their triangles.** The import's 1,724 models hold 490,321 triangles and 26.4 MB of vertex positions; the America command center's main model holds 534 (measured 21 October 2026 from the imported package's GLBs).

## Other games

- **StarCraft II.** A unit's selection radius, apart from its model, sets how far from the model a click still selects it ([SC2Mapster](https://www.sc2mapster.com/forums/development/triggers/150741-is-it-possible-to-make-a-unit-type-bigger-ingame), [Liquipedia](https://liquipedia.net/starcraft2/StarCraft_II_Editor)).
- **League of Legends.** Each unit has a selection area of its own for clicks ([wiki](https://wiki.leagueoflegends.com/en-us/Targetable)).
- **OpenRA.** Each actor has screen rectangles, its `Interactable` bounds, and `Selectable.Priority`, 10 by default; under the cursor the actor of the highest priority minus its rectangle's center's distance from the cursor in pixels wins, the priority lower by 30 for an ally, 60 for a neutral and 90 for an enemy ([`SelectableExts.cs`](https://raw.githubusercontent.com/OpenRA/OpenRA/bleed/OpenRA.Game/SelectableExts.cs), [`Selectable.cs`](https://raw.githubusercontent.com/OpenRA/OpenRA/bleed/OpenRA.Mods.Common/Traits/Selectable.cs)).
- **Spring's collision volume.** A unit's collision volume, apart from its model, serves line-of-fire checks and the mouse click that selects one unit; a 2017 change let a game give a selection volume of its own apart from it ([CollisionVolumes](https://springrts.com/wiki/Gamedev:CollisionVolumes), [Mantis 5258](https://springrts.com/mantis/view.php?id=5258)).
- **Spring and Rise of Nations.** A selection prefers mobile units over buildings when both are under it ([Spring forum](https://springrts.com/phpbb/viewtopic.php?p=38138)).

## Exact picking

- **Triangles.** A ray against each mesh's triangles, after a box test, is exact to what is drawn; Bevy's `MeshRayCast` does it, sorting the meshes the ray's boxes meet and exiting early, but it runs its query in parallel and needs each mesh's data in the main world (`bevy_picking` 0.20, `mesh_picking/ray_cast`).
- **An ID buffer.** Rendering each object's id and reading the pixel under the cursor is exact and cheap for millions of triangles, but costs a pass and a readback that lags a frame or more; one report's frame time rose from 4 to 11 ms without buffering the readback ([gamedev.net](https://gamedev.net/forums/topic/703589-dx11-render-target-picking/?page=1), [Unity discussion](https://discussions.unity.com/t/mrt-approach-to-picking-as-an-alternative-to-ray-casting/856860), [Babylon.js](https://forum.babylonjs.com/t/new-feature-gpu-picking/51106)).
- **Neither widens a thin unit,** so a game that picks exactly still needs a tolerance for small units.

## Bevy's picking

- **Backends feed one pipeline.** A backend reads each pointer's ray from `RayMap` and sends `PointerHits`: the entities hit, each with its depth, and an `order`; hits of a higher `order` come before those of a lower one whatever their depth, and within one `order` by depth (`bevy_picking` 0.20, `backend.rs`, `hover.rs`).
- **Blocking.** An entity whose `Pickable::should_block_lower` holds, the default, hides the hits under it from the hover, so a UI node, whose backend sends a higher `order` than the camera's, hides the world under it.
- **One hovered set.** `HoverMap` gives the entities hovered for each pointer, an unordered map; `PointerInteraction::sorted_entities` sorts them by depth alone.
