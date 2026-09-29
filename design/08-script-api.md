# Campfire — Script API (MOBA kit)

Draft, derived from the reference packages in `source/packages/moba/`: six heroes, the player spells and the 3v3 mode. Every name below is used there; a name the packages do not need is not here. Nothing runs yet.

## Rules

- **Data first.** What the kit can do from data, it does: held modifiers, charges, charged casts, projectiles, areas, auras, shields. Scripts describe only what is special about an effect.
- **Hooks by name.** The kit calls a script function by its hook name. Hook names differ between roles (ability, modifier, mode, AI), so one file can serve an ability and the modifiers it applies.
- **Handles read, `ctx` changes.** Handles are read-only, except `.state` and a modifier's `.stacks`, which a call may write and read back. Every other change goes through `ctx` and is queued: the call is all-or-nothing ([Scripting](02-engine-core.md#scripting)).
- **Default source.** `ctx` knows the acting unit: the caster, a projectile's or area's source, or a modifier's source. Damage, heals, modifiers, projectiles and areas come from it unless a call names another.
- **New entities at once.** A handle to an entity created in a call is usable in that call, including `.state`; the entity appears when the call commits.
- **Event order.** A call's effects apply in call order. The hooks they trigger are queued first-in first-out and run after all of the call's effects. A hook of a modifier that is gone by then is skipped. A chain longer than an engine constant fails with `script_error`.
- **Dead and despawned units.** A handle keeps returning the unit's last values, and `alive` is false. Effects on a dead unit do nothing, except `respawn`. A modifier whose source despawned keeps that source's last stats.
- **Values are live.** `ctx.p` resolves when it is read, with the current rank and the current stats of the source.
- **Pure hooks.** `calc_damage` returns a value; its `ctx` refuses effects.
- **Queries.** Lists are sorted by stable id; `nearest_visible` sorts by distance, then id. Dead and untargetable units are never returned. `find` includes units hidden from the caller's team, for effects on an area; `find_visible` and `nearest_visible` return only what that team sees, for choosing a target.
- **Numbers.** [Game Scripting](03-game-scripting.md#numbers). `num(i)` makes a `Num` from an integer. `Num` has `min`, `max`, `clamp`, `round`, `floor`, `ceil`; `round` and the others return integers.
- **Randomness** comes only from `ctx.chance` and `ctx.pick`, both from the secret stream. Crits are rolled by the kit.
- **Time** is in milliseconds, rounded up to whole ticks.
- **Rhai names.** Rhai's reserved words cannot be names, including `spawn` and `match`.

## Filters

One set of words selects units everywhere: `targeting`, projectile `hits`, area `affects`, aura `affects`, and every query. A filter is a relation, `enemies`, `allies` or `all`, and an optional tag after a colon: `enemies:hero`, `allies:hero`, `enemies:creep`. Allies include the unit itself; the `neutral` team is an enemy of both sides. Heroes carry the tag `hero`.

## Data files

Arrays in kit fields and in the params of an ability are per rank: 5 entries for a basic ability, 3 for the ultimate. Anywhere else an array is a list.

**Param values** (`[params]`, read as `ctx.p.<name>`; an unknown name is an error):

| Form | Example | Value |
| --- | --- | --- |
| Integer | `slow_ms = 2000` | the integer |
| Decimal string | `radius = "3.0"` | a `Num` |
| Array | `slow = ["0.15", "0.20", "0.25", "0.30", "0.35"]` | the entry for the current rank |
| Scaling table | `{ base = [80, 120, 160, 200, 240], ap = "0.65" }` | `base + per_level × level + Σ ratio × stat` of the source |

Scaling keys: `base`, `per_level`, and a stat ratio each for `ad`, `bonus_ad`, `ap`, `max_health`, `bonus_health`, `armor`, `magic_resist`. A number field anywhere in data may be `{ param = "<name>" }`, resolved in the same way.

**Hero** (`data/hero.toml`): `name`, `role`, `resource` (`mana` or `energy`), `passive` (a modifier id), `slots` (four ability ids; the last is the ultimate), `[attack]` (`range`, `windup_ms`, `projectile_speed`; no speed means melee), `[stats]` (`stat = { base, per_level }`; the value at level `n` is `base + per_level × (n − 1)`), `[abilities.<id>]`, `[modifiers.<id>]`.

**Ability:**

| Field | Meaning |
| --- | --- |
| `script` | Script file, if the ability needs one |
| `targeting` | `none`, `point`, `direction`, or a filter for a unit target |
| `range` | Meters, or `"global"` |
| `cooldown_ms`, `cost`, `cast_time_ms` | Cost is in the hero's resource |
| `clamp_to_range` | A target beyond range is moved in, instead of the caster walking |
| `toggle` | `{ cost_per_attack }` or `{ cost_per_second }`; the kit turns it off at zero resource and at death |
| `channel` | `{ duration_ms, tick_ms }`; starts after `on_cast`; calls `on_channel_tick` |
| `hold` | Modifier the kit holds while the toggle is on or the channel runs |
| `charges` | `{ max, recharge_ms }` |
| `charge` | `{ max_ms }`: a charged cast. `on_cast` runs at release, with `ctx.charge` from 0 to 1 and `ctx.origin` and the target as they were when the charge began |
| `passive_modifier` | Modifier held while the ability has a rank; with `passive_while_ready`, only while it is off cooldown |
| `projectile` | `speed`, `width`, `range` (default: the ability's), `stop_on_hit`, `once_per_cast`, `hits`, `sight_radius`, `collide` |
| `area` | `radius`, `delay_ms`, `duration_ms`, `affects`, and `inside = { self, allies, enemies }`: modifiers held on units while they are inside |
| `[params]`, `[projectile_state]` | Script values; the state of each projectile the ability fires |

**Modifier:** `script`, `duration_ms` (absent: until removed), `interval_ms`, `stacks_expire_ms`, `reapply` (`refresh` by default, `stack`, `ignore`), `stats` (per stack), `states`, `shield` (the modifier ends when the shield is spent), `aura` (`{ radius, affects, modifier }`), `[params]`, `[state]`. A unit holds at most one instance of each modifier id from each source.

**Mode** (`data/mode.toml`): `script`, `assist_window_ms`, `[inputs]` (name and type of each player input: `string`, `string_list`), `[state]`, `[params]`, `[modifiers]`. An input that does not match its type never reaches the script. **Units** (`data/units.toml`): `[units.<id>]` with `ai`, `think_ms`, `tags`, `true_sight`, `attack`, `stats`, `params`.

**State types:** `int`, `num`, `bool`, `string`, `entity`, `entity_list`, `pos`, `vec`; each with a `default` and, for mode state, `sync`.

**Stats:**

| Stats | Combine | Limits |
| --- | --- | --- |
| `health`, `health_regen`, `resource`, `resource_regen`, `attack_damage`, `ability_power`, `armor`, `magic_resist`, `armor_pen`, `magic_pen`, `move_speed`, `physical_block` | Sum | — |
| `attack_damage_pct`, `ability_power_pct`, `attack_speed_pct`, `move_speed_pct`, `healing_received_pct`, `damage_dealt_pct` | Sum | ≥ −1 |
| `attack_speed` | Sum, then × (1 + `attack_speed_pct`) | ≤ 2.5 attacks per second |
| `crit_chance`, `armor_pen_pct`, `magic_pen_pct`, `life_steal`, `spell_vamp` | Sum | 0 to 1 |
| `cooldown_reduction` | Sum | 0 to 0.4 |
| `slow` | Strongest only | 0 to 0.99; `slow_immune` ignores it |

Move speed is `move_speed × (1 + move_speed_pct) × (1 − slow)`, at least an engine floor. `life_steal` heals the source for attack damage dealt and `spell_vamp` for ability damage; `healing_received_pct` scales every heal.

**States:** `stunned`, `rooted`, `silenced`, `disarmed`, `airborne`, `stealthed`, `untargetable`, `slow_immune`.

## Handles

| Handle | Reads |
| --- | --- |
| Unit | `pos`, `team`, `owner`, `level`, `alive`, `is_hero`, `target`, `attack_range`, `health`, `max_health`, `stat(name)`, `params` (the unit type's, unresolved), `spawn_pos`, `lane`, `unit_type`, `has_tag(tag)`, `has_modifier(id)`, `is_enemy_of(unit)`, `can_see(unit)`, `recent_attackers(ms)` |
| Modifier `m` | `carrier`, `source`, `stacks` (writable), `state` (writable) |
| Projectile | `source`, `pos`, `distance` flown, `state` (writable) |
| Area | `source`, `pos` |
| Damage `d` | `source`, `target`, `amount`, `kind` (`physical`, `magic`, `true`), `attack`, `crit`, `extra` (an attack from `ctx.attack_hit`), `ability` (`""` when none) |
| Position, vector | `distance_to`, `direction_to`, `rotated_deg`, `+`, `-`, `*` |

## `ctx`

| Group | Calls |
| --- | --- |
| Values | `p.<name>`, `range`, `charge`, `origin`, `state` (mode), `map` (`lanes`, `neutral_spawns`), `teams` (playing teams) |
| Queries | `find(of, pos, radius, filter)`, `find_visible(of, pos, radius, filter)`, `nearest_visible(of, radius, filter)`, `heroes()`, `heroes(team)`, `units_tagged(tag)`, `enemy_team(team)`, `hero_available(player, id)` |
| Random | `chance(p)`, `pick(list)` |
| Combat | `damage(target, amount, kind)`, `heal(unit, amount)`, `restore(unit, amount)` (the unit's resource), `attack_hit(target)` |
| Modifiers | `add_modifier(unit, id)` or `(unit, id, duration_ms)`, returns the modifier; `remove(handle)` removes a modifier, projectile or area |
| Crowd control | `stun(unit, ms)`, `slow(unit, fraction, ms)`, `knock_up(unit, ms)`, `knock_back(unit, from, distance, ms)` |
| Movement | `dash(unit, to, speed)`, `teleport(unit, pos)` |
| Projectiles | `projectile(from, to)` or `(from, to, overrides)`: the ability's `projectile` data; `to` a direction flies a line, a unit homes, a position flies to it; `from` a unit or a position |
| Areas | `area(pos)`: the ability's `area` data |
| Vision | `reveal(pos, radius, ms)` for the source's team |
| Abilities | `reduce_cooldown(unit, id, ms)`, `reduce_cooldowns(unit, fraction)` (basic abilities), `add_charge(unit, id)` |
| Progress | `add_resource(player, name, amount)`, `add_xp(hero, amount)` |
| Orders (AI) | `order_attack(unit, target)`, `order_move(unit, pos)`, `order_follow_lane(unit)`, `order_reset(unit)` (walk home, heal, drop target) |
| Mode | `timer(name, ms, repeat, data)`, `end(result)`, `spawn_heroes()`, `spawn_unit(type, team, pos)`, `spawn_wave(team, lane, types)`, `respawn(unit, ms)`, `choose_hero(player, id)`, `choose_spells(player, ids)` |

`ctx.p` reads, in an ability, its params; in a modifier, the modifier's params and then those of the ability that applied it; in a mode or AI script, the mode's params.

## Hooks

| Role | Hooks |
| --- | --- |
| Ability | `on_cast(ctx, caster, target)` (target: a unit, a position or `()`), `on_channel_tick(ctx, caster)`, `on_dash_end(ctx, unit, target)`, `on_projectile_hit(ctx, proj, target)`, `on_projectile_end(ctx, proj)`, `on_area_trigger(ctx, area, units)` |
| Modifier | `on_interval(ctx, m)`, and the carrier's events: `on_attack(ctx, m, target)`, `on_attack_hit(ctx, m, d)`, `on_damage_taken(ctx, m, d)`, `on_kill(ctx, m, victim)`, `on_takedown(ctx, m, victim)` |
| Mode | `on_match_start(ctx)`, `on_mode_input(ctx, player, name, value)`, `on_timer(ctx, name, data)`, `on_player_join(ctx, player)`, `on_player_leave(ctx, player)`, `on_unit_died(ctx, unit, killer, assisters)`, `calc_damage(ctx, d)` |
| AI | `think(ctx, unit)` |

A hero passive is a modifier the hero always carries. The kit finds takedown participants once, with the mode's `assist_window_ms`: they receive `on_takedown`, and the mode receives them as `assisters`.

## Checks at package load

A package loads only when all of these pass:

- Every data file matches its schema, and every per-rank array has one entry for each rank.
- Every script is referenced by data. Every function named like a hook is a hook of a role the script serves, so a misspelled hook is an error, not a hook that never runs.
- Every modifier id, `ctx.p` name, `{ param }` reference, stat, state, filter and damage kind that a script or data file names exists. Scripts are read with `AST::walk`, from Rhai's `internals` feature.
- Every `ctx` call is one this API defines.

## Found while writing the scripts

1. Passives are permanent modifiers, and carrier events go to modifier scripts: one mechanism for passives, buffs, marks and, later, items.
2. Toggles, channels, charged casts, charges and granted passives are data. The kit holds their modifiers, so no script can leave one behind.
3. Projectiles and areas are data; areas hold modifiers on the units inside, which needs no tick script.
4. Values must be live: resolving them once when a modifier is applied froze passives at their level-1 values.
5. Stats need a rule for how they combine and limits; slows as negative move speed added up without limit.
6. Modifier identity is (id, source), with a rule for applying one again.
7. Handles to dead or despawned units, and hooks of removed modifiers, need defined behavior.
8. The kit, not the mode, decides who assisted.
9. Player inputs are untrusted and typed in data.
10. Rhai's default limits differ between debug and release builds, so the engine sets every limit itself ([Scripting](02-engine-core.md#scripting)).
11. `spawn` is reserved in Rhai.
12. A package in the workspace names its dependencies by path; the built package names them by fingerprint.

## Open questions

- [ ] Camp monsters respawn one by one; whole camps may be better.
- [ ] Kill streaks and assist rules beyond an even gold split.
- [ ] Items: the same modifier and ability model, not yet written.
