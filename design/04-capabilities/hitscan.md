# Hitscan

Shots that hit at once, tested against hitboxes, with lag compensation. Damage goes through `combat`, so a ray can hit a unit that takes orders and has a hitbox as well as a character.

## Weapons and hits

- Weapon data: fire rate, damage, falloff, penetration, recoil, spread.
- Spread comes from the sim RNG on the server only. The client predicts the shot effect, not where it lands: a client that knows the spread can cancel it.
- Rays test per-body-part hitboxes. Hitboxes follow a simplified animation pose computed in the sim.
- Bullets that travel and drop are `projectiles` falling under gravity, rewound to the firing tick like a ray.

## Lag compensation

The sim keeps a short history of hitbox poses as part of its state and tests each shot at the tick named in the shooter's input frame. The rewind has a limit the host sets (200 ms by default): a frame that names an older tick is tested at the limit, so a client cannot claim a stale view to hit a target that already reached cover. That tick is in the recorded input and the history is sim state, so the verifier gets the same hit. It is capability code, not the network layer's lag compensation, because the verifier runs no network layer.
