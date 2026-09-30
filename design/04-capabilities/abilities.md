# Abilities, projectiles and areas

## Abilities

The capability handles the mechanics; the script only describes the effect.

- **Fields:** targeting, range, cooldown, cost, cast time, toggles, channels, charges, charged casts, granted passives, projectiles, areas, ranks; see [Script API](../08-script-api.md#data-files).
- **Checks:** every cast is validated when it starts: the slot's ability is learned and off cooldown, its cost is affordable, and its target is a living unit of the relation the targeting names, within range on the ground plane (visibility comes with fog of war). Cast and channel time run, and interrupts apply.
- **All or nothing.** A cast resolves in the Hit stage, after the tick's attacks strike. Its checks run again, without the range; then `on_cast` runs, and the cost, the cooldown and the effects the script queued apply together. If the checks fail or the script fails, none of them apply, and the failure goes to the tick's script errors.
- **Values** of the capability fields may be one or one per rank, like params; times become whole ticks when the ability loads, rounded up. Until levels and stats exist, a scaling param counts every unit at level 1 with every stat at 0.
- A unit casts through an order (`orders`), or through a button in a `character` input frame: the same ability works for a MOBA hero and a first-person character.

## Projectiles and areas

| Primitive | Does |
| --- | --- |
| Projectile | Linear, homing, or falling under gravity; calls `on_projectile_hit`. Ranged attacks fire one. |
| Area | Circle, delayed or lasting; holds modifiers on the units inside; cones and lines are projectiles |

A projectile flies in the Hit stage, a fixed distance each tick, from the tick after it fires; flights run before attacks strike, and the tick's launches after. A homing one flies at its target's position of that tick, and strikes when it reaches it, in that tick's Resolve; one whose target dies or despawns before it lands ends without a hit. Projectiles take stable ids in the order of their source's id, so every run numbers them alike.

## Sent to clients

Cooldowns and charges go to the owner, or the team, as the mode sets. The client predicts the start of its own casts; effects appear when the server confirms them.
