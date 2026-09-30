# Abilities, projectiles and areas

## Abilities

The capability handles the mechanics; the script only describes the effect.

- **Fields:** targeting, range, cooldown, cost, cast time, toggles, channels, charges, charged casts, granted passives, projectiles, areas, ranks; see [Script API](../08-script-api.md#data-files).
- **Checks:** every cast is validated (range, cooldown, cost, a valid and visible target); cast and channel time run, interrupts apply, and cooldown and cost are spent.
- A unit casts through an order (`orders`), or through a button in a `character` input frame: the same ability works for a MOBA hero and a first-person character.

## Projectiles and areas

| Primitive | Does |
| --- | --- |
| Projectile | Linear, homing, or falling under gravity; calls `on_projectile_hit`. Ranged attacks fire one. |
| Area | Circle, delayed or lasting; holds modifiers on the units inside; cones and lines are projectiles |

A projectile flies in the Hit stage, a fixed distance each tick; a homing one whose target dies or despawns before it lands ends without a hit.

## Sent to clients

Cooldowns and charges go to the owner, or the team, as the mode sets. The client predicts the start of its own casts; effects appear when the server confirms them.
