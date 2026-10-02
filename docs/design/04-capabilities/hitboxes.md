# Hitboxes

## Mechanism

Hits tested against the parts of a body, with lag compensation: the `ray` delivery, a shot that hits at once, and the `sweep` delivery, a shape swept through the air over a windup, as a sword or an axe swings ([Deliveries](actions.md#deliveries)). A shooter's gun is an attack action whose delivery is a ray; an action RPG's swing is one whose delivery is a sweep. Their damage goes through `combat`, so a ray or a sweep can hit a unit that takes orders as well as a character.

## Data

A unit type's `hitboxes = { set = "<name>" }` names its hitboxes, each a body part. An action with `delivery = "ray"` adds `falloff` (damage by distance), `penetration`, `recoil` and `spread`; one with `delivery = "sweep"` adds its `shape` (a capsule's radius and length) and its `arc` (the path the shape takes over the windup, from the unit's view). The ammunition of a gun is a pool of its item ([Items](items.md)).

## Rules

- Rays and sweeps test per-body-part hitboxes. Hitboxes follow a simplified animation pose computed in the sim. The damage records the body part and the direction of the hit in `d.hit`, and the mode's `calc_damage` gives them their weight: a headshot, a sneak attack from behind and a blocked swing from the front are rules of the mode.
- **Sweeps.** A sweep tests its shape along its arc in each tick of the windup's last part, from the unit's pose of that tick; each unit it meets is hit once a swing, in the order it meets them, and a unit of the `blocks` it meets, such as a wall, ends it.
- **Spread** comes from the sim RNG on the server only. The client predicts the shot's effect, not where it lands: a client that knows the spread can cancel it.
- Bullets that travel and drop are projectiles falling under gravity, rewound to the firing tick like a ray.
- **Lag compensation.** The sim keeps a short history of hitbox poses as part of its state and tests each ray and each sweep at the tick named in the attacker's input frame. The rewind has a limit the host sets (200 ms by default): a frame that names an older tick is tested at the limit, so a client cannot claim a stale view to hit a target that already reached cover. That tick is in the recorded input and the history is sim state, so the verifier gets the same hit. It is capability code, not the network layer's lag compensation, because the verifier runs no network layer.

## State and derived

- **State:** the history of hitbox poses, as far back as the rewind limit.
- **Derived:** each tick's poses, from positions and animation.

## Network

A client predicts its own shots' and swings' effects; hits and damage come from the server.

## Cost

Each ray costs a test against the hitboxes near its line, at the tick it names; each sweep a test of its shape against the hitboxes near its arc, each tick it moves; the history costs each unit with hitboxes a pose a tick.

## Genres

Counter-Strike's and a battle royale's guns; Skyrim's and an action RPG's melee; an RTS or MMO unit with hitboxes may be hit by them in a mixed game.
