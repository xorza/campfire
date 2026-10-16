# Hitboxes

## Mechanism

- **Rays and sweeps against body parts.** Planned: the `ray` delivery, a shot that hits at once, and the `sweep`, a shape swept over a windup, test per-body-part hitboxes posed in the sim; their damage goes through `combat`, so a ray hits a unit that takes orders as well as a character.
- **The mode weighs the part.** The damage records the body part and direction, and `calc_damage` weighs a headshot or a blocked swing.
- **Spread from the server's RNG.** The client predicts a shot's effect, not where it lands.
- **Lag compensation in the sim.** The sim keeps a short history of poses as state and tests each ray at the tick its attacker's input names, at most the host's rewind limit back, 200 ms by default, so a stale view cannot hit a target already in cover, and the verifier gets the same hit as the history is state.
