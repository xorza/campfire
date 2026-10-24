# Crowd movement

How established engines move units through crowds, out of factories and round each other, and how
this engine does it today, read on 2026-10-24. Facts only; [Crowds](../design/04-capabilities/navigation.md#crowds)
decides from them.

## This engine, 2026-10-24

- **Three layers.** A long route on the pathing grid round static bodies, which ignores units; a
  short route, A* in a window of 17 × 17 cells round the walker, round the units that stand, and,
  for a walker stuck 150 ms, round the walkers it touches, each counted half its reach to the
  walker's left; and collision, which parts each overlapping pair along the line between their
  centers (`steering/mod.rs`, `collider/mod.rs`).
- **Who takes an overlap.** A walker that runs into a unit that stands takes the whole overlap; two
  walkers share it. A walker beside another is so pushed sideways, whatever way it faces, as a
  vehicle is.
- **Who yields.** Only an idle friend that a walker stuck 150 ms touches ahead of it steps aside,
  by a destination of its own; a walker that has not stalled yet moves none, and a unit near the
  route but not touching stays.
- **Every cell costs the same.** The planner prices a straight step 10 and a diagonal 14
  (`route_planner/mod.rs`), so no route goes round a crowd: the crowd is not on the grid.
- **A trained unit's exit.** It spawns at the free cell nearest its producer's exit and walks to
  its rally point (04-capabilities/production.md, A producer may name its exit and rally); the units
  that stand at the rally point are blockers to its short route, and none of them moves for it
  until it stalls against one.
- **No kind of motion.** A unit type has a `MoveStep` and a `TurnStep`; nothing says whether a
  unit may be moved sideways.

## Command & Conquer: Generals Zero Hour

From the released source, as [GeneralsX](https://github.com/fbraz3/GeneralsX) builds it.

- **Units are priced in the search, not hidden from it.** A cell an ally that stands holds costs
  `3 × COST_DIAGONAL` more, a cell near a moving ally the same, and an ally's goal cell
  `3 × COST_ORTHOGONAL` more for a vehicle and `COST_ORTHOGONAL` for infantry; a route marks
  itself blocked by an ally when it crosses one (`AIPathfind.cpp:6363-6407`).
- **A route moves the allies on it.** After a route that allies block, `Pathfinder::moveAllies`
  tells each ally that holds a cell of it to move away, with `aiMoveAwayFromUnit`; a dozer and a
  harvester clear their route always. It moves no ally that moves, attacks, uses an ability or is
  busy, and recurses at most 2 deep (`AIPathfind.cpp:10217-10300`).
- **Infantry passes infantry.** `moveAllies` leaves infantry in the way of infantry, as "infantry
  can walk through other infantry"; infantry moves a vehicle only when the vehicle blocks it
  (`AIPathfind.cpp:10276-10285`).
- **Moving away is a move.** `aiMoveAwayFromUnit` puts the unit in a temporary state,
  `AI_MOVE_OUT_OF_THE_WAY`, which plans a path off both the pusher's path and the last pusher's,
  and drives it, so a vehicle turns and drives aside; asked again by the same unit while blocked,
  it ignores collisions for 2 s (`AIUpdate.cpp:3214-3260`).
- **A factory's exit.** A unit leaving a factory follows its exit path, may path through units,
  and moves the allies that stand at its destination away, with
  `moveAlliesAwayFromDestination`, which walks the cells on the line to the destination
  (`AIUpdate.cpp:1693-1700`, `AIPathfind.cpp:6962-7008`).
- **A blocked and stuck unit plans again** (`AIUpdate.cpp:430, 509-513`).

## 0 A.D.

From `CCmpUnitMotion_System.cpp` of [0 A.D.](https://github.com/0ad/0ad), and
[rP26245](https://code.wildfiregames.com/rP26245).

- **Pushing is a displacement, in fixed point.** Each turn, pairs of units within a push distance
  each take a positional push, summed per unit, then damped and applied; all of it in
  `entity_pos_t` fixed point, in a lockstep game.
- **Units part before they touch.** The push distance is the pair's clearances, scaled by 5/7 to
  circles, times 8/5, plus 5/2 for two that move and 2 for one that stands; within 5/8 of it the
  push is the strongest, at most 5/2.
- **Lighter units move more.** Each side's push is the other's weight over its own, capped at 4 ×
  half the turn.
- **Pressure.** Each push adds pressure to both units, kept at 0.6 of itself a turn; a pushed unit
  moves slower and is pushed less, damped by at most 160 of its pressure, so a crowd settles.
- **Pushes do not cross walls.** A push into a blocked place is dropped and the unit marked
  obstructed.
- **What it bought.** "It makes movement of groups of units much smoother overall"; a player's
  test found it does away with about 90 % of overlaps, not all
  ([forum](https://wildfiregames.com/forum/topic/64466-why-can-units-overlap/)).
- **Units that fail to move.** A long path after 3 failed short paths; the two pathfinders
  alternate every 6 failures, report "very obstructed" after 10, and stop at 35
  ([UnitMotion](https://docs.wildfiregames.com/pyrogenesis/namespaceanonymous__namespace_02CCmpUnitMotion_8h_03.html)).

## Others

- **ORCA.** Each neighbour gives a half-plane of safe velocities, and the agent takes the
  velocity nearest the one it wants, by a small linear program; motion stays smooth, thousands of
  agents in milliseconds; no global planning, and the reference library is floating point
  ([ORCA](https://gamma.cs.unc.edu/ORCA/), [RVO2](https://gamma.cs.unc.edu/RVO2/documentation/2.0/whatsnew.html)).
  It assumes agents that may take any velocity; a unit that turns at a rate needs an extension.
- **DetourCrowd.** Recast's crowd samples candidate velocities in rings round the wanted one and
  scores each by the wanted direction, the current velocity, a side bias and the time to impact
  with circles of position, velocity and radius ([Recast](https://recastnav.com/group__crowd.html)).
- **Supreme Commander 2.** A flow field over sectors of tiles, with local steering
  ([Game AI Pro 360, ch. 7](https://www.taylorfrancis.com/books/9780429100277/chapters/10.1201/b16725-29));
  a community reading finds no dynamic density field in it
  ([forum](https://wildfiregames.com/forum/topic/16018-supreme-commander-2-pathfinding/)).
- **StarCraft II.** Blizzard's talk on its navigation, "AI Navigation: It's Not a Solved Problem,
  Yet" (GDC 2011), is not public; second-hand accounts give a constrained Delaunay navmesh, one
  route for a group, separation steering, and units that push through their own rather than go
  round ([GameDev.net](https://gamedev.net/forums/topic/648438-how-to-do-starcraft-2-pathfinding/)).
  Hold position keeps a unit where it stands
  ([Liquipedia](https://liquipedia.net/starcraft2/Micro_(StarCraft))).
- **A crowded goal.** The A* Pathfinding Project stops a unit whose goal's surroundings pass a
  density, as 50 %, rather than push on, as units sent to one point otherwise circle it
  ([forum](https://forum.arongranberg.com/t/how-to-improve-units-pathfinding-when-running-to-the-same-point/11269)).
- **Spring.** A unit whose 2D position stays the same for a while cancels its move
  ([Mantis 2188](https://springrts.com/mantis/view.php?id=2188)).
- **Dota 2.** This engine's avoidance takes Dota 2's three kinds, a long route, a short route and
  collision ([Navigation](../design/04-capabilities/navigation.md#movement)), under which units
  block units and none moves another.
