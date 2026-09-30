# MOBA kit

## Units and orders

**A unit** is kit components (position, collision shape, health, team, stats, modifiers, abilities, current order) plus script state, declared in the unit type's data file.

**Input format: orders.** Players, bots and AI all issue the same orders: move, attack, cast (ability, target), stop, hold. The kit executes them (pathfinding, attacking, casting).

## Attacks and deaths

- **Attack order.** `attack` names its target by stable id; an order on a unit that is not a living enemy is ignored. Different teams are enemies, and the neutral team is an enemy of both sides.
- **Range** is measured on the ground plane, exactly, with no square root. Out of range the unit walks to its target; in range it stops.
- **Windup and period.** An attack starts once the unit is ready, and strikes when its windup ends. The next attack may start one period after this one started. A move, or an attack on another target, cancels a windup and spends nothing, so the unit may attack again at once; after the strike, in the back-swing, moving is free. Range counts only at the start: a strike lands unless its target died or despawned.
- **Ranged attacks** strike at the end of the windup for now; projectile travel comes with the projectile primitive.
- **Damage.** All strikes of a tick apply together after collision, in the order of their source's stable id, so each follows from the state before any of them: two units can kill each other in one tick. The mode's `calc_damage` comes with scripts; until then an attack deals its damage as is.
- **Death.** A unit at zero health dies at the end of the damage step. A hero stays, dead, for the mode to respawn; it takes no orders and is no target. Any other unit despawns.
- **Towers** keep their target while it lives and stays in range, and otherwise take the nearest enemy in range, the lower stable id on a tie; a tower in its windup keeps its target. This stands in for the tower AI script until scripts run, which also prefers creeps and defends heroes.
- **Creep waves** spawn on each lane for the first side, then the second, on a fixed timer that stands in for the mode's `spawn_wave` timer. A creep walks its lane's waypoints, the first side's forward and the second side's backward, while it has no target; until creep AI runs, it never takes one.
- **Time** in the kit is in ticks. Data in milliseconds and rates per second become ticks when a package loads, rounded up.

## AI

AI scripts drive creeps, neutrals and bosses.

- Set per unit type in its data. `think(ctx, unit)` runs every `think_ms` (e.g. 250 ms, rounded up to whole ticks) and issues orders.
- Think times are staggered by unit id, so units do not all think on the same tick.
- Only game queries (units in radius, nearest visible enemy, recent attackers) and the sim RNG; results are sorted by unit id. Orders go through `ctx`.
- The reference game ships creep, tower and camp AI as ordinary scripts.

## Abilities

The kit handles the mechanics; the script only describes the effect.

- **Kit fields:** targeting, range, cooldown, cost, cast time, toggles, channels, charges, charged casts, granted passives, projectiles, areas, ranks; see [Script API](../08-script-api.md#data-files).
- **Kit:** validates every cast (range, cooldown, cost, valid and visible target), runs cast and channel time, handles interrupts, applies cooldown and cost.
- **Passives are modifiers** a hero always carries; the carrier's events (attack, hit, damage, kill, takedown) go to modifier scripts.

| Primitive | Does |
| --- | --- |
| Projectile | Linear or homing; calls `on_projectile_hit` |
| Area | Circle, delayed or lasting; holds modifiers on the units inside; cones and lines are projectiles |
| Modifier | Buff, debuff or passive, one per source: duration, stacks, stat changes, states, shields, auras, periodic `on_interval` |
| Damage | Amount and type; the final formula (armor, resistances) is the mode hook `calc_damage` |

## Space and tick rate

Positions are 3D, but gameplay is on the ground plane: collision, pathfinding and vision use x and z, and y comes from the map's height data. High ground is a vision rule in the grid, not a height test. Default tick rate: 30 Hz.

## Pathfinding and vision

- Grid backend: A* for the long route; shared local steering avoids units and allows body blocking.
- Creep lanes are waypoints in map data.
- Vision: grid fog of war; each unit reveals cells within its sight range, blocked by terrain. Brush cells block sight from outside the brush.
- Stealth: a stealthed unit is visible only to its team and to enemies with true sight over it (vision wards, towers, consumables). Wards are units with sight range and no collision.

## Network defaults

| Data | Sent to |
| --- | --- |
| Position, health, visible modifiers, current animation | Everyone who sees the unit |
| Mana, cooldowns, resources such as gold | Owner, or team (set by the mode) |

Clients predict their own hero's movement and cast start; damage and deaths appear only when the server confirms them.

## Tick steps

One MOBA tick, in fixed order. It fills in the [core tick pipeline](../03-game-scripting.md#tick-pipeline):

1. **Apply inputs** (core + kit): this tick's player and bot orders become current orders.
2. **AI think** (kit + script): units due this tick issue orders.
3. **Execute orders** (kit): pathfinding, steering, attacks, cast start.
4. **Collision** (core): resolve overlaps with the mode's backend.
5. **Ability effects** (kit + script): `on_cast`, projectiles, areas.
6. **Modifiers and damage** (kit + script): `on_interval`; damage via the mode's `calc_damage`; units at zero health die.
7. **Mode hooks** (script): due `on_timer`, then `on_unit_died` in death order; `ctx.end` ends the match.
8. **Vision** (core): update grid fog of war for each team.

## Examples

The reference packages in `source/packages/moba/` are the examples: six heroes, the player spells, and the 3v3 mode with creep, tower and camp AI. The API they use: [Script API](../08-script-api.md).
