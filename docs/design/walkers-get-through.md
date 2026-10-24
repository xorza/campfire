# Walkers get through

A slice's proposal ([Documents](../AGENTS.md#documents)): its decisions fold into
[Navigation](04-capabilities/navigation.md) when it ends, and this document goes.

At its end, a player who sends a dozer past a column of idle dozers in a base sees the column
drive ahead out of its way, and no unit of a base start stands for good with a destination: each
walker either gets where it goes or stops within seconds. The facts are in
[Crowd movement](../research/crowd-movement.md).

## Decisions

- **A walker progresses only when it moves along its way.** A tick counts toward being stuck
  unless the walker's place moved at least half its step along the leg it walks, its net move
  projected on the leg's way; a detour, a short route or a new route keeps the count, and only such a
  move resets it. As 0 A.D.'s `CCmpUnitMotion` counts a failed movement each turn its move is
  obstructed or a push takes it away from where it moves, and resets the count only on an
  unobstructed move. Refused: a move of half a step in any way, under which a hacker pushed back
  each tick by a crawler's personal space stood 0.77 m short of its goal for 6,240 ticks; a
  count a detour resets, under which a Red Guard took a detour it could not walk every 5 ticks
  and never asked a new route.
- **A walker stuck by its goal has arrived.** A walker stuck within its own radius of its route's
  last waypoint has arrived there, not short, as the A* Pathfinding Project's agents reach their
  end within `endReachedDistance`. Refused: 5 asks of 1 s each, which leave it jittering at its
  goal for 5 s; no tolerance, under which a goal within another unit's personal space is never
  reached.
- **A unit spawns only where it can walk out.** `room_near` takes, for a unit that walks, only a
  place whose cell its clearance holds open, in the region of the open cell nearest the search's
  point, as Zero Hour's `adjustToPossibleDestination` keeps a goal in the unit's own zone and its
  spawn spread takes only clear cells. A unit that cannot walk keeps the room test alone.
  Refused: room for the body alone, under which a supply truck spawned in a pocket of its base
  whose ways out are narrower than its body and never moved.
- **A route in sight is a line.** A route whose goal the walker sees, by the exact test of the
  static bodies, the walls and the priced cells a route's smoothing makes, is the straight line
  to it, before any search of cells, as a route's smoothing keeps such a line. Refused: the cells
  alone, under which a Red Guard in a gap that holds no cell open to its clearance took short
  routes to its own cell's center, which it could not reach, though it saw its goal.
- **A friend yields through the friends that yield.** A friend's search for a place clear of the
  walker's way passes through the idle friends the walker may move, as cells it may cross and not
  as blockers, and may end on one; each it would pass or end on steps aside in turn for the
  friend's own walk, one step deeper, 2 deep at most, so a column in a corridor drives ahead,
  its front first, as Zero Hour's `moveAllies` moves the allies on an ally's way to its place and
  OpenRA's nudge moves an idle neighbour when no cell is free. Refused: friends as blockers of
  the search, under which a dozer column in a corridor was hemmed in and the dozer behind it
  arrived short; a push of idle friends by the walker, as StarCraft II pushes idle units, which a
  unit that drives does not take (04-capabilities/navigation.md, A unit that drives yields by
  driving).

## Checks

- A walker bound for a goal within an idle unit's personal space arrives once it is stuck within
  its radius of the goal, and one whose goal a stuck unit holds stops after its 5 asks.
- A walker that takes detours it cannot walk asks its route again after 1 s.
- `room_near` refuses a pocket whose ways out are narrower than the unit's body, and takes the
  place nearest outside it.
- A walker in a gap that holds no open cell walks to a goal it sees.
- A wide walker behind a column of two wide idle friends in a corridor with no side room arrives,
  the column driving ahead of it.
- The Zero Hour check's bases play has no walker that stands with a destination for 1,000 ticks,
  and its stalls and ends are named in each batch's commit.
