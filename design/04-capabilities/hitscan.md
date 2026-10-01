# Hitscan

## Mechanism

The `ray` delivery ([Deliveries](actions.md#deliveries)): a shot that hits at once, tested against hitboxes, with lag compensation. A shooter's gun is an attack action whose delivery is a ray; its damage goes through `combat`, so a ray can hit a unit that takes orders and has hitboxes as well as a character.

## Data

An action with `delivery = "ray"` adds `falloff` (damage by distance), `penetration`, `recoil` and `spread`. A unit type's `hitscan = { hitboxes = "<set>" }` names its hitboxes, each a body part. The ammunition of a gun is a pool of its item ([Items](items.md)).

## Rules

- Rays test per-body-part hitboxes. Hitboxes follow a simplified animation pose computed in the sim. The damage records the body part in `d.hit`, and the mode's `calc_damage` gives it its weight: a headshot is a rule of the mode.
- Spread comes from the sim RNG on the server only. The client predicts the shot's effect, not where it lands: a client that knows the spread can cancel it.
- Bullets that travel and drop are projectiles falling under gravity, rewound to the firing tick like a ray.
- **Lag compensation.** The sim keeps a short history of hitbox poses as part of its state and tests each shot at the tick named in the shooter's input frame. The rewind has a limit the host sets (200 ms by default): a frame that names an older tick is tested at the limit, so a client cannot claim a stale view to hit a target that already reached cover. That tick is in the recorded input and the history is sim state, so the verifier gets the same hit. It is capability code, not the network layer's lag compensation, because the verifier runs no network layer.

## State and derived

- **State:** the history of hitbox poses, as far back as the rewind limit.
- **Derived:** each tick's poses, from positions and animation.

## Network

A client predicts its own shots' effects; hits and damage come from the server.

## Cost

Each ray costs a test against the hitboxes near its line, at the tick it names; the history costs each unit with hitboxes a pose a tick.

## Genres

Counter-Strike and a battle royale's guns; an RTS or MMO unit with hitboxes may be hit by them in a mixed game.
