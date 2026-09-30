# Campfire — Capabilities

The engine has no genres. It has **capabilities**: each is one mechanism, such as health and damage, units that take orders, a first-person character or grid fog of war. A game package declares the capabilities it uses, and its mode scripts and data make the genre. A MOBA, an RTS, an FPS and a game that mixes all three are the same kind of package: the reference MOBA is only the first.

A capability is native code: components, systems that run in the tick, backends, a data schema for unit types, commands, and the script calls and hooks it adds. Capabilities follow the core's determinism rules and ship in engine releases; packages cannot add native code, because the verifier must run only code the release pins. A new combination of capabilities needs no release; a new mechanism does.

## Capabilities

| Capability | Adds | Doc |
| --- | --- | --- |
| `combat` | Health, attacks, the damage pipeline, deaths, kill credit | [Combat](combat.md) |
| `stats` | Stats, how they combine, modifiers, states such as stun | [Combat](combat.md#stats-and-modifiers) |
| `abilities` | Targeting, range, cooldown, cost, cast and channel time, charges, toggles | [Abilities](abilities.md) |
| `projectiles`, `areas` | Linear, homing and falling projectiles; circles that hold modifiers | [Abilities](abilities.md#projectiles-and-areas) |
| `orders` | Units that take orders: move, attack, cast, stop, hold, queues, groups and formations; AI `think` | [Control](control.md#orders) |
| `character` | Units a player drives directly: per-tick input frames, capsule controller | [Control](control.md#character) |
| `hitscan` | Rays against hitboxes, lag compensation, weapon data | [Hitscan](hitscan.md) |
| `navigation` | Grid A*, navmesh, local steering, waypoint paths such as lanes | [Navigation](navigation.md) |
| `vision` | Grid fog of war, stealth, 3D occlusion, dynamic blockers such as smoke, hearing, relevance | [Vision](vision.md) |
| `items` | Inventories, equipment, world items to pick up and drop, shops; an item grants stats, abilities or a weapon | [Items](items.md) |
| `progression` | Experience, levels, learning ranks of abilities, talents, veterancy | [Progression](progression.md) |
| `interaction` | Using objects: doors, containers, plant and defuse, capture points, talking to NPCs, entering vehicles and buildings | [Interaction](interaction.md) |
| `production` | Build queues, construction with footprints on the grid, harvesting, tech requirements, rally points | [Production](production.md) |
| `physics` | Vehicles, rigid bodies, heightmap terrain | [Physics](physics.md) |
| `persistence` | Saved characters and world state, dormancy, quests | [Persistence](persistence.md) |

Which capabilities make which genre, and what each genre adds in scripts: [Genres](genres.md).

## Mode vocabulary

The core has no genre words and no genre lists. A mode declares, in its data:

- **Damage kinds:** physical and magic for a MOBA; bullet and explosive for a shooter; fire, frost and shadow for an MMO.
- **Stats:** the numbers its units have. The core knows only the stats a capability reads, such as move speed and attack speed.
- **Resources:** of units (mana, energy, rage, ammo) and of players (gold, minerals, supply).
- **Teams and slots,** as now.

Neutral core terms: a player's **avatar** (a hero, a soldier, a character), a **loadout** (what a player picks before spawning), a **spawn group** (a wave, a squad), a **path** (a lane, a patrol route). A package names them in its own words.

## Layers

Capabilities share one vocabulary, so they meet in one match: a hitscan ray and a MOBA projectile damage the same `combat` health. The dependencies form a fixed graph with no cycle:

- **Base:** `sim` (positions, stable ids, randomness, the state hash, the tick rate) and collision; then the core under every script: unit types, the one script host and its tick budget, the shared types (team, owner, path), and the units as scripts see them. A capability adds its fields to that view, so the core names no capability.
- **`combat` and `stats`:** health, damage, deaths, stats and modifiers.
- **Everything else** builds on those: `projectiles` and `hitscan` deal damage through `combat`, `abilities` apply modifiers through `stats`, `orders` issue attacks and casts.

The capabilities are modules of one crate; a capability with a heavy dependency, such as physics, gets its own crate. Only the declared capabilities' systems run, so an unused one costs nothing.

## Tick stages

The engine fixes the stages of a tick, and each capability puts its systems into them. Within a stage, a capability orders its systems against those of the capabilities it builds on: in Act, `control` chases a target before `combat` starts the attack. Two systems with no order and conflicting access fail the schedule build, so no order is left to chance.

| # | Stage | Runs |
| --- | --- | --- |
| 1 | Inputs | Core: the tick's commands reach the capabilities that own them |
| 2 | Think | AI `think` of the units due this tick issues orders |
| 3 | Act | Current orders and intents: attack windups, cast starts, path requests, character intents |
| 4 | Move | Steering, the character controller, dashes |
| 5 | Collide | Core: the mode's collision backend resolves overlaps |
| 6 | Hit | Attack strikes, hitscan rays, projectiles and areas, `on_cast` |
| 7 | Resolve | Modifier intervals, damage through the mode's `calc_damage`, deaths |
| 8 | Mode | Due timers, then the capabilities' events in the order they happened; spawns; `ctx.end` |
| 9 | Vision | `vision` marks what each team sees, from the map's grid |

## Commands

A player input's payload is a list of commands. Each command names the capability that owns it by the capability's index in the engine's fixed list, one byte, and holds that capability's format: an order, a `character` input frame. A mode input goes to `mode`, the owner every match has and no manifest declares. The list only grows: a new capability takes the next index, so an old log decodes the same. One packet can carry a first-person frame and an order to a squad. A command of a capability the mode did not declare, or one that does not decode, is ignored: a client can send anything.

## Control

Control is a relation from a player slot to entities, of one of two kinds:

- **Direct:** the player drives the entity with `character` input frames, and the client predicts it.
- **Orders:** the entity follows the player's orders (`orders`); an order names the units it goes to, so one player can order many.

A player can hold both kinds at once: a first-person commander drives a character and orders squads. AI scripts issue the same orders; bots send the same commands as players.

## Unit types

A unit type in data is a set of sections, one for each capability it uses:

```toml
[units.siege_tank]
tags = ["vehicle"]
stats = { health = { base = 900 }, attack_damage = { base = 60 }, attack_speed = { base = "0.4" }, move_speed = { base = "3.0" } }
combat = { attack = { range = "9.0", windup_ms = 400, projectile_speed = "14" } }
orders = { ai = "scripts/tank_ai.rhai", think_ms = 250 }
hitscan = { hitbox = "tank" }
```

The team and the owner come from the spawn, not the unit type.

A section of a capability the mode did not declare fails the package load. Time is in milliseconds and rates are per second; they become whole ticks when the package loads, rounded up, so a unit type behaves the same at any tick rate to within one tick.

## Script API

`ctx`, the handles and the hooks are made of the declared capabilities' parts: a mode without `combat` has no `ctx.damage` and no `unit.health`. The package load checks refuse a call, a field or a hook of a capability the mode did not declare. [Script API](../08-script-api.md) lists each capability's part.

## Tick rate

The mode picks its tick rate within the range its manifest allows: an FPS wants 64 to 128 Hz, a MOBA 30, an MMO or a large RTS 10 to 20. Every capability works in ticks at any rate, and each states what its worst tick costs, so a mode can see what its combination costs before it ships.

## Open questions

- [ ] A mechanism no capability has needs an engine release. Scripted per-entity systems would let creators add one without a release, at a cost per tick and under a budget; not designed yet.
- [ ] Which capabilities two modes share decides how much of a hybrid is tested before it ships; `det-ci` runs the reference MOBA and the genre proofs, and other combinations are tested by their authors.
