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

## A friend with no room to yield, 2026-10-24

A replay of a client match on `tournament_desert`, China's dozer #143 sent north behind two idle
dozers, each of radius 19.72 m, the circle round its 34 × 20 m box.

- **The route goes through the friends.** The long route runs along x = 1015 through the idle
  dozers #155 and #156, each priced as a friend the walker may move.
- **The first step aside is measured against the first stretch.** At tick 1,242, #155 steps
  10 m off the walker's diagonal first stretch, to (996.4, −433.2); the route then turns north
  and #155 is on it again.
- **No room at the side.** Its next step aside, to (975.6, −435.0), has no place it may stand:
  the nearest is where it stands, (995, −435), so it stays.
- **The detour aims inside a unit.** The short route's target, the last waypoint in its window,
  (1015, −405) or (1025, −395), lies within the reach of #156 or of the Tank Hunter #159, so no
  short route reaches it, and 311 of the walker's 313 short routes are dropped.
- **The new route is the old one.** Each new route the stuck walker asks prices #155 as before
  and runs through it again, until the player orders another goal.
- **No place clear of the way.** With friends that search outward for a place clear of the
  walker's route ahead within their window of ±8 cells, #155 finds none: of the 289 cells of its
  window, 81 can hold its body past the base's buildings, 23 of those can be reached from it past
  the units that stand, and no center of those is 39.4 m off #143's route; a window three times
  as wide finds none either. #155 is hemmed in, #143's new route walls it, and #143 arrives short
  at (1015, −485).

## Friends that yield, measured, 2026-10-24

- **The Zero Hour check.** With friends that yield to a place clear of the walker's route and
  routes of stuck walkers that wall those that cannot, the bases play ends in tick 9,096 with 332
  stalls, against 10,839 and 445 before; the attack play in tick 22,650 with 121 stalls, against
  25,163 and 163 with the step aside to a unit's own place refused alone.
- **A detour as near as it can.** A short route that cannot reach its waypoint, taken when it ends
  nearer the waypoint than the walker, held a walker pressed against a busy friend at a
  corridor's mouth moving back and forth each tick: never stuck, it never planned again nor
  stopped in 240 ticks; refused to a stuck walker, the waypoints it passed cleared its count of
  stuck asks, so it never stopped either.
- **Walls for every firm unit.** A stuck walker's route that walls every firm friend, not those
  hemmed in alone, left a hero of the 3v3 match at level 5, below the 6 every hero reaches.
- **A search over the cells of other units.** A search that took cells other standing units hold
  sent a trained unit 1 m through its rally's crowd onto a member's place, and left a pair of
  bodies overlapping past the slack.

## Walkers that stand with a destination, 2026-10-24

The Zero Hour check's bases play, traced each tick.

- **A truck shut in by its base.** China's supply truck #225, of radius 18.38 m, spawns by
  `room_near` at (1131.45, −529.02) in a pocket of the base's buildings 39 m deep, whose ways out
  are 10, 13 and 18 m wide: it never moves. No cell of the pocket is open to its clearance, so
  its route leaves from the open cell nearest it, (1175, −495), past a building's corner; it
  asks its route 5 times, stops, and is sent to the same goal again at once, each 186 ticks.
- **A goal another unit holds.** Red Guard #158, of radius 7 m, is bound for (1138.85, −541.45),
  12.5 m from the truck, within their two radii. It stands in an 18 m gap between two
  buildings that holds no cell open to its clearance; each 5 ticks it is stuck, its short route
  ends on its own cell's center, which it cannot reach, and the detour starts its stuck count
  again, so it never asks a new route.
- **A goal within another's personal space.** Hacker #162, of radius 10 m, is bound for
  (1115, −395), 39.43 m from the idle troop crawler #172, of radius 23.77 m, within their
  personal space of 5/4 of their radii, 42.21 m. Each tick it steps 0.67 m toward the goal and
  the crawler's push takes it back; it stands 0.77 m short for 6,240 ticks, never stuck by the
  count, as each detour it takes once stuck starts the count again.

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
- **A mover is not bounced by another mover.** `PhysicsBehavior::onCollide` lets an object with an AI decide by `AIUpdateInterface::processCollision`, which applies no force between two units that move on the ground, as "AI objects move under their own initiative, not by getting bounced" (`PhysicsUpdate.cpp:1270-1280`, `AIUpdate.cpp`, `processCollision`). Read 29 October 2026.
- **Blocked is a cone ahead.** `AIUpdateInterface::blockedBy` holds a mover blocked by a unit within 45° of its facing, 33.75° for a unit that stands, and by none behind it past 90°, none it can crush, and none while it is within a cell of its goal; two that cross off that cone are blocked only when they close on each other, and then only the one of lower path priority (`AIUpdate.cpp`, `blockedBy`).
- **A blocked mover brakes to the speed of the one ahead.** `calculateMaxBlockedSpeed` caps the mover's speed at the blocker's speed away from it along the line between them, over the share of its own facing along that line; zero for a blocker that comes toward it; 55 % of that for two of one formation (`AIUpdate.cpp`, `calculateMaxBlockedSpeed`).
- **A deadlock steps the lower priority aside.** A mover blocked by one that is blocked by it in turn, both facing their ways, moves away from it by `aiMoveAwayFromUnit` when it has the lower path priority: a dozer over all, a vehicle over infantry, then the one ahead along their two facings, then the lower id; a mover blocked by one that stands, facing its way, is blocked and stuck, and plans again; a vehicle that a soldier blocks tells the soldier to move away (`AIUpdate.cpp`, `processCollision`, `hasHigherPathPriority`).
- **A blocked move plans again after 2 s.** `AIInternalMoveToState::update` plans a new path when the unit is blocked and stuck, or blocked more than `2 × LOGICFRAMES_PER_SECOND` frames (`AIStates.cpp:1834-1840`).
- **An ally moves away anywhere clear of the path.** `Pathfinder::getMoveAwayFromPath` searches
  outward from the ally's cell over the cells it may stand in, and takes the first whose box,
  the ally's radius plus the mover's, crosses no segment of the mover's path nor of the last
  pusher's (`AIPathfind.cpp:10318-10490`).

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
- **Moving and stopped units apart.** `Push` returns at once for a pair of one unit that moves and
  one that stands: "moving entities will push moving entities, but not stopped ones, and
  vice-versa"; the members of one formation push idle units. A push under `MinimalPushing`, 0.2 by
  default, is dropped, and one that turns a unit well off its move marks it obstructed
  ([CCmpUnitMotion_System.cpp](https://github.com/0ad/0ad/blob/master/source/simulation2/components/CCmpUnitMotion_System.cpp),
  read 29 October 2026).
- **The push is along the centers.** The push runs along the line between the two units' centers,
  averaged over the turn's start and end; only two that crossed paths take a nudge across it.
- **Pushes do not cross walls.** A push into a blocked place is dropped and the unit marked
  obstructed.
- **What it bought.** "It makes movement of groups of units much smoother overall"; a player's
  test found it does away with about 90 % of overlaps, not all
  ([forum](https://wildfiregames.com/forum/topic/64466-why-can-units-overlap/)).
- **Units that fail to move.** A long path after 3 failed short paths; the two pathfinders
  alternate every 6 failures, report "very obstructed" after 10, and stop at 35
  ([UnitMotion](https://docs.wildfiregames.com/pyrogenesis/namespaceanonymous__namespace_02CCmpUnitMotion_8h_03.html)).

## Others

- **0 A.D.'s failed moves.** `CCmpUnitMotion` counts a failed movement each turn its move is
  obstructed, and only an unobstructed move resets the count; a new path keeps it. It backs up
  on the 10th, informs at the 35th that the move failed, and switches between its short and long
  pathfinders on the way. A unit a push takes away from where it moves, by the dot product of
  its intended and its net move, counts as obstructed; moving units push only moving ones, and
  stopped units only stopped ones ([CCmpUnitMotion.h](https://github.com/0ad/0ad/blob/main/source/simulation2/components/CCmpUnitMotion.h),
  [CCmpUnitMotion_System.cpp](https://github.com/0ad/0ad/blob/main/source/simulation2/components/CCmpUnitMotion_System.cpp)).
- **An end close enough.** The A* Pathfinding Project's agents reach the end of their path
  within `endReachedDistance`, and get as close as they can to a destination they cannot reach
  ([AIPath](https://arongranberg.com/astar/documentation/stable/aipath.html)).
- **Zero Hour keeps a destination in reach.** `Pathfinder::adjustToPossibleDestination` takes a
  goal cell in the unit's own zone, its connected region, that `checkDestination` passes, else
  the nearest such cell (`AIPathfind.cpp:5655`); a spawn spread takes only clear cells
  ([GeneralsGameCode #1882](https://github.com/TheSuperHackers/GeneralsGameCode/pull/1882)).

- **OpenRA.** A blocked move waits `WaitAverage` ticks, tells the blocker, then plans again with
  `BlockedByActor.All`, under which every actor blocks; with no route it ends the move with
  `CompleteDestinationBlocked` ([Move.cs](https://github.com/OpenRA/OpenRA/blob/bleed/OpenRA.Mods.Common/Activities/Move/Move.cs)).
  An idle friendly actor told it blocks queues a `Nudge` to a random free adjacent cell, and with
  none moves an idle neighbour; a busy one only marks itself blocking
  ([Mobile.cs](https://github.com/OpenRA/OpenRA/blob/bleed/OpenRA.Mods.Common/Traits/Mobile.cs)).
- **Ask, then plan again.** Practitioners ask a blocking idle unit to move and plan a new route
  only when it cannot, and price standing units, busy ones more, so a route goes round them
  first ([GameDev.net](https://gamedev.net/forums/topic/567708-practical-path-finding-for-rts/),
  [The Fertile Crescent](https://lincread.itch.io/the-fertile-crescent/devlog/53276/improving-pathfinding)).
- **0 A.D.'s short routes reach only so far.** Units fail to go round large obstacles, as the
  short pathfinder's range is the limit ([ticket 5624](https://trac.wildfiregames.com/ticket/5624)).

- **ORCA.** Each neighbour gives a half-plane of safe velocities, and the agent takes the
  velocity nearest the one it wants, by a small linear program; motion stays smooth, thousands of
  agents in milliseconds; no global planning, and the reference library is floating point
  ([ORCA](https://gamma.cs.unc.edu/ORCA/), [RVO2](https://gamma.cs.unc.edu/RVO2/documentation/2.0/whatsnew.html)).
  It assumes agents that may take any velocity; a unit that turns at a rate needs an extension.
- **DetourCrowd.** Recast's crowd samples candidate velocities in rings round the wanted one and
  scores each by the wanted direction, the current velocity, a side bias and the time to impact
  with circles of position, velocity and radius ([Recast](https://recastnav.com/group__crowd.html)).
  After the moves, it parts overlapping agents by position in 4 passes, each agent taking half of
  each overlap times 0.7, so the velocity planning keeps overlaps rare and the positional pass
  small ([DetourCrowd.cpp](https://github.com/recastnavigation/recastnavigation/blob/main/DetourCrowd/Source/DetourCrowd.cpp),
  `dtCrowd::update`, read 29 October 2026).
- **OpenRA's entry.** `Enter` takes the target's place again each tick, walks by
  `MoveToTarget`, and enters by `MoveIntoTarget` once `CanEnterTargetNow` holds, with
  `RetryIfDestinationBlocked`
  ([Enter.cs](https://github.com/OpenRA/OpenRA/blob/bleed/OpenRA.Mods.Common/Activities/Enter.cs),
  read 29 October 2026).
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

## Walkers that cross, measured, 2026-10-29

A probe of the navigation capability: walkers of a half meter that step a quarter meter a tick, on
half-meter cells, of one team, each column's walkers 1.1 m apart; each walker's offset across the
line from its start to its goal, read each tick.

- **Two columns of three that cross at right angles.** The east column's leader slides 2.89 m
  across its line while its facing never changes, and ends 0.52 m off it; the north column's
  second walker slides 3.27 m, reverses across its line 8 times and turns 10 times in a few ticks
  as it is stuck; the walkers move across their lines 2.8 m to 8.6 m in all, against
  0.81 m net at most. With walkers that drive, a walker turns up to 14 times, and two overlap by 0.054 m.
- **Three abreast, 1.1 m apart, the same way.** Pushed apart to their personal space of 1.25 m,
  the outer two move 0.27 m across their lines and back.
- **A slower walker ahead.** A walker of a quarter meter behind one of an eighth on the same line
  follows it at the gap collision leaves, never passing it, and arrives a tick after it.
- **A column through a gap of 1.6 m, and six through a corridor of 2.4 m.** No overlap; walkers
  reverse across their lines up to 3 times, and step back along them up to 3 times.
- **The cause.** `Collider::push` pushes each pair within their personal space of which one walks
  along the line between their centers, a quarter of the intrusion a tick, whatever way each
  walks, and `Collider::resolve` shares an overlap of two walkers between them; a walker's step
  then goes on along its leg, so a walker beside another slides across its way.

## The Zero Hour check with entries and blocking, measured, 2026-10-29

The check's plays on the import of 29 October 2026, run four times: as the golden holds them,
before the walk into a unit; with the walk into a unit alone; with the brake in place of the
push; and with the gait's rule, a soldier that moves no vehicle aside but on a route through its
team's units or once stuck, and a mover braked to a stop counted stuck, on the import of
30 October 2026, which hides a carried model's flashes.

| Play | Golden | Walk into a unit | And the brake | And the gait |
| --- | --- | --- | --- | --- |
| Bases: ended, stalls | tick 13,906, 531 | tick 11,407, 512 | tick 9,607, 208 | tick 9,681, 737 |
| Attack: ended, stalls | tick 14,101, 101 | tick 14,101, 101 | tick 13,947, 79 | tick 14,272, 69 |
| Transports: boarded, fired, fell | 884, 10, 128 | 866, 24, 89 | 866, 24, 89 | 866, 24, 89 |
| Garrison: garrisoned, outranged | 824, 1,351 | 824, 1,351 | 975, 1,320 | 985, 1,310 |
| Fight: shot, hit, fight | 905, 915, 2,621 | 905, 915, 2,621 | 913, 923, 2,637 | 946, 956, 2,662 |

- **The gait's rule, by its exceptions, 30 October 2026.** A soldier that never moves a vehicle:
  the transport, combat and garrison plays never end, the bases play ends in tick 13,671 with
  1,611 stalls. With the stuck exception alone: every play ends, the bases play in tick 10,352
  with 860 stalls, the attack play in 14,040 with 82, the garrison in 909. With the route's
  exception alone: the combat and garrison plays never end, the bases play ends in tick 9,953
  with 734 stalls. With both: the table's last column.
- **The entry play, 30 October 2026.** China's two Troop Crawlers let their 16 Red Guards out in
  57 ticks; the first crawler's 8 walk back into it in 60, the crawler standing where it stood;
  then the second's 8 into a Helix, which flies 245 m to a place it lands on and takes 5, the
  other 3 refused, in 837; the 3 into the Overlord's bunker once bought in 75, the Overlord
  standing still. Before the gait's rule, an Overlord and the second crawler were moved aside by
  the Red Guards that walked past them; with neither exception, a Red Guard stood braked to a stop
  behind the second crawler, which the Helix had moved, for the rest of the play, as a mover
  braked to a stop counted no stuck tick.
- **The bases play's stalls, by cause, 30 October 2026.** Four runs, each of a 1 s probe of the
  stuck units: as the golden holds it, tick 9,681 and 737 stalls; with the gait's rule turned off,
  tick 15,093 and 849; with a mover braked to a stop counted as no stuck walker, as before, tick
  11,243 and 394; with both, Batch 2's tick 9,607 and 208. In the first, 282 of the samples are
  China's supply trucks, of which two, 167 and 168, stand within 9 m of each other by the supply
  center at (1,033, −401) for 96 and 126 samples, each walking to its own goal; with the mover
  counted as before, no supply truck is among the samples. GLA's Angry Mob members are the most of
  the rest: 555 samples with the gait's rule, 333 without.
- **The garrison's slower Rebel.** Of the two Rebels, the one that starts among the base's units
  walks east 90 m round them before it turns for the building, and arrives in tick 985 against
  the other's 750; the golden's 824 was measured before the brake, with no probe of its way.
