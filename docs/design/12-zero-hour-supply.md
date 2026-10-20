# Zero Hour supply

A proposal for one slice. When it ends, each lasting decision moves into the document of the system it governs, the plan moves to `PLAN.md` as it starts, and this document is deleted ([A slice's proposal is temporary](../../AGENTS.md#documents)). Its facts are in [Zero Hour supply](../research/zero-hour-supply.md).

**At the slice's end** a China or GLA player in a Zero Hour skirmish builds a supply center, whose free gatherer fills at the map's nearest dock and carries its boxes home, and the player's money rises box by box as in the original; a right-click on a dock sends a selected gatherer to it.

## Decisions

### Production

- **A gather takes its load a unit at a time.** A gather's `unit` is what each windup takes, up to its `take`, so a trip from a nearly empty node is short; with no `unit`, one windup takes the whole load, as now. Practice: Zero Hour's box each `SupplyWarehouseActionDelay`, OpenRA's bales. Refused: a whole trip's windup, which pays a full trip for the last box of a dock.
- **A drop-off takes its time.** A gather's `deliver` is how long a worker stands at the drop-off before its load joins its player's, Zero Hour's `SupplyCenterActionDelay`. Refused: a delay on the drop-off, as Zero Hour's differs by gatherer, 150 ms for a worker and 400 ms for a truck.
- **An empty node may stay.** A node's `empty` is `goes`, as now, or `stays`, as Zero Hour's supply dock stays and its pile goes (`DeleteWhenEmpty`).
- **A producer may start its gatherers.** A producer's `gathers` sends each gatherer that leaves its exit, trained or freed, to the free node nearest it within its gather's `bounce`, as a unit leaving Zero Hour's `SupplyCenterProductionExitUpdate` is forced into wanting. Refused: OpenRA's `SearchOnCreation` on the gatherer, which would start a GLA worker that a command center trains, as Zero Hour does not.
- **A building may come with its units.** A producer's `free` lists the unit types that leave its exit once, as its site completes, as OpenRA's `FreeActor` gives a refinery its harvester and Zero Hour's `SpawnBehavior` of `OneShot` a supply center its gatherer. Refused: the mode script's `on_site_complete`, which can spawn a unit but not send it through an exit or order it.

### The import

- **A box is money.** A dock's node holds `StartingBoxes` × `ValuePerSupplyBox`, 75 in the shipped `GameData.ini`, and a gatherer's `take` and `unit` are `MaxBoxes` and one box of it, so a delivery pays what the original's does. Refused: boxes as a resource of their own, which the center would have to convert.
- **The USA's income waits for aircraft.** The Chinook, the USA's only gatherer, flies, and the import trains no unit that does not walk, so the USA's supply center gathers nothing until a slice brings aircraft; 09 lists it as lost.

### The client

- **A right-click on a node gathers.** A selected unit whose gather takes the node's resource casts it there, and one with a load right-clicked on its own drop-off returns the load, as Zero Hour's dock order does.

## Plan

Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. Production's gathering options (M)

`production`, its scripted match, 06's reference.

- The gather's `unit` and `deliver`, a node's `empty`, and a producer's `gathers` and `free`, each with its default today's behaviour, read and checked at the load.
- **Check:** a test of each option against hand-computed ticks and amounts, each failing with the option's default; the gather loop's bench case runs once; the reference regenerates.

### 2. The import of supply (M)

`import`, the rules package.

- Docks and piles become nodes, the China and GLA centers drop-offs that start and free their gatherers, the supply truck and the worker gatherers, each with the original's numbers; `GameData.ini`'s `ValuePerSupplyBox` is read; the Chinook's gather is dropped as an import loss.
- **Check:** fixture tests of each module's numbers and of the box value; `cargo drive run zero-hour`, on request, moves its golden for these types alone.

### 3. A dock order and the check (M)

`client`, `zero-hour-check`, 09.

- A right-click on a node or a drop-off gathers or returns; the check plays China on Tournament Desert, builds a supply center, and pins the money at each delivery of its free truck; 09 gains the slice's losses: the USA's income, the upgrades' boost, a dock's visible boxes, the approach queue, and the regroup of a gatherer with no dock.
- **Check:** the click's test; the check's golden holds each delivery's tick and amount.

## Checks

- Each batch ends with `cargo drive gate` passing, and a golden that moves names its batch and its cause.
- The slice ends when the user plays it in the client, and the Zero Hour check keeps it working.
