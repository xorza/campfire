# Base defenses

A proposal for one slice. When it ends, each lasting decision moves into the document of the system it governs, and this document is deleted ([A slice's proposal is temporary](../../AGENTS.md#documents)); its plan is in `PLAN.md`. Its facts are in [Zero Hour base defenses](../research/zero-hour-base-defenses.md).

**At the slice's end** a player builds a Patriot battery, a tunnel network and a Stinger site and sees each fire at an enemy that comes near, the site's three Stinger soldiers with missiles, each soldier back 30 s after it dies and the tunnel's two defenders 10 s after; and trains Technicals, each of one of three chassis, Inferno cannons and, after a Strategy Center, Tomahawks, and sees them fire.

## Decisions

### Production

- **A building may stand on a circle.** A build's body is a box or a circle, and its placement tests that shape against the walls, the map's bounds and the bodies, and its builders walk to the shape's nearest point, as Zero Hour's pathfinder gives a cylinder structure a circle of its radius (`Pathfinder::classifyObjectFootprint`). Refused: a circle's bounding box, which blocks the corners that the original leaves free and makes the tunnel network 50 m square.
- **A requirement may be any one of several types.** An entry of `requires.units` is a type or a list of types, and a list holds when the player holds any one of them, as Zero Hour's prerequisite line `Object = A B` (`UNIT_OR_WITH_PREV`) and StarCraft II's requirement nodes of "or". Refused: OpenRA's prerequisite tokens that buildings provide, which make every building of an alternative name a token it provides, a second name for one fact.
- **A building may keep spawns.** A producer's `spawns = { unit_type, count, replace_ms }` leave its exit as its site completes, each that dies comes back `replace_ms` later, and they die with it, as Zero Hour's `SpawnBehavior` of `SpawnedRequireSpawner` keeps the Stinger site's soldiers; a producer of no exit sends them from the free cell nearest its center. Refused: the mode script's `on_unit_died`, which every game would write again; `free` units, which leave once and are never replaced.
- **A train may make one of several types.** A train's `variants` lists the unit types it makes in place of its own, one drawn for each unit from the producer's own random stream, as Zero Hour's `ThingFactory::newObject` draws a `BuildVariations` entry. Refused: the first variant alone, which drops two of the Technical's looks; variants of the look alone, as the chassis differ in their bodies, which the sim reads.

### Combat

- **A splash may spare its source's kin.** A splash's `spares_kin` spares each ally of its source of the source's unit type, as Zero Hour's `NOT_SIMILAR` spares an equivalent template of the source's ally (`dealDamageInternal`), so a row of Tomahawks does not wound itself. Refused: sparing every unit of the type, under which an enemy Tomahawk takes nothing, where the original wounds it.

### The import

- **A cylinder structure builds as a circle.** A structure of a cylinder geometry that is not small builds on a circle of its `GeometryMajorRadius`, as it already stands on one; a small structure, as the demo trap, has no body and stays dropped.
- **A prerequisite line converts to a list.** A line of several objects becomes one entry that any of them holds; a line of a science stays dropped, as no science is imported.
- **A clip of one shot is a period.** A weapon of `ClipSize = 1` converts to a period of its `ClipReloadTime` and no clip, as each shot of the original empties the clip and reloads it from the shot, so its `DelayBetweenShots` never counts (`Weapon::privateFireWeapon`); the engine counts a period and a reload alike from the attack's start, so the conversion is exact.
- **`NOT_SIMILAR` converts; `NOT_AIRBORNE` stays dropped.** `NOT_SIMILAR` becomes `spares_kin`; `NOT_AIRBORNE` keeps the drop until the engine has units above the ground.
- **A kept `SpawnBehavior` converts to spawns.** One of no `OneShot` becomes the producer's `spawns` of its first `SpawnTemplateName`, `SpawnNumber` and `SpawnReplaceDelay`; a `OneShot` stays a free unit.
- **`BuildVariations` converts to a train's variants.** A train of a type of `BuildVariations` makes its variations, each a unit type of its own object, with its own look.
