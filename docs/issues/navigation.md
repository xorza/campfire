# Navigation

Design: [Navigation](../design/04-capabilities/navigation.md). Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

- Each creep of a wave's group asks for its own route, from nearly the same cell to the same lane end, so the 3v3's 24 creeps of a wave plan about four distinct routes six times each, up to the planner's work limit for a tick and into the next tick ([Hot stages](../design/13-benches.md#hot-stages), Move).

## Ready

- **Plan: M3.** `Collider::part` takes each contact's distance with `u128::isqrt`, 4.0 % of `collision/crowded`, where `math`'s exact root takes less than half the time ([Integer roots](../design/14-integer-roots.md)).
