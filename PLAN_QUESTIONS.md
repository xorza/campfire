# Plan questions

Decisions the execution of [PLAN.md](PLAN.md) could not take alone.

## Units of another team that stand on a route

- **Found in:** W1, by the Zero Hour check's attack play: China's supply trucks #156, #161 and
  #163, of radius 18.38 m, stand at (1222.31, −1233.89) for up to 16,890 ticks. Their route's
  waypoint (1225, −1275) lies within the reach of #118, a civilian taxi of team 8 that stands for
  the whole match. Each new route a stuck truck asks runs through the taxi again, as
  (04-capabilities/navigation.md, A route prices the units on it) prices no unit of another team,
  and a stuck walker's route walls only friends hemmed in.
- **Zero Hour:** its pathfinder blocks a cell a non-ally that stands holds when the walker cannot
  crush it (`enemyFixed`, `AIPathfind.cpp:5203`).
- **Options:**
  1. **A stuck walker's route walls every unit that stands and that it may not move**, of any
     team, as OpenRA plans again with `BlockedByActor.All`; the first route keeps pricing no unit
     of another team. Cost: a hero stuck behind enemy creeps routes round them or arrives short.
  2. **Units of no player's team are priced as held**, as neutral cars and civilians never move
     for a walker. Cost: enemies that stand stay unpriced, so the same jam stays possible against
     an enemy that stands.
  3. **Both.**
- **Recommendation:** 1, as it is OpenRA's practice, needs no team rule, and only touches routes
  of walkers already stuck.
- **Blocked:** nothing in the plan; the slice's last check, no walker that stands with a
  destination for 1,000 ticks in the Zero Hour check, fails until this is decided.
