# Hitboxes

## Mechanism

Planned. Hits tested against the parts of a body, with lag compensation: the `ray` delivery, a shot that hits at once, and the `sweep` delivery, a shape swept through the air over a windup, as a sword swings ([Deliveries](actions.md#deliveries)). A gun is an attack whose delivery is a ray; a swing one whose delivery is a sweep. Their damage goes through `combat`, so a ray can hit a unit that takes orders as well as a character.

## Data

A unit type's `hitboxes = { set = "<name>" }` names its hitboxes, each a body part. A `ray` action adds `falloff` (damage by distance), `penetration`, `recoil` and `spread`; a `sweep` action its `shape` (a capsule's radius and length) and `arc` (the shape's path over the windup, from the unit's view). A gun's ammunition is a pool of its item ([Items](items.md)).

## Rules

- Rays and sweeps test per-body-part hitboxes, which follow a simplified animation pose computed in the sim. The damage records the body part and the direction in `d.hit`, and the mode's `calc_damage` weighs them: a headshot, a sneak attack, a blocked swing.
- **Sweeps** test their shape along their arc in each tick of the windup's last part, from the unit's pose of that tick; each unit met is hit once a swing, in the order met, and a unit of the `blocks` it meets, as a wall, ends it.
- **Spread** comes from the sim RNG on the server only; the client predicts a shot's effect, not where it lands.
- Bullets that travel and drop are falling projectiles ([Gravity](../13-bodies-uses-shop-gravity.md#gravity)), rewound to the firing tick like a ray.
- **Lag compensation.** The sim keeps a short history of hitbox poses as state, and tests each ray and sweep at the tick its attacker's input frame names, at most the host's rewind limit back (200 ms by default), so a stale view cannot hit a target already in cover. The tick is in the recorded input and the history is state, so the verifier gets the same hit: capability code, as the verifier runs no network layer.

## State and derived

- **State:** the history of hitbox poses, back to the rewind limit.
- **Derived:** each tick's poses, from positions and animation.

## Network

A client predicts its own shots' and swings' effects; hits and damage come from the server.

## Cost

Each ray a test against the hitboxes near its line; each sweep a test of its shape near its arc, each tick it moves; each unit with hitboxes a pose a tick.

## Genres

Counter-Strike's and a battle royale's guns; an action RPG's melee; any unit with hitboxes in a mixed game.
