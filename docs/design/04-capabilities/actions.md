# Actions

## Mechanism

- **One pipeline for every action.** An attack, a cast, a use, a build, a train and a gather run one pipeline of checks, time, delivery, effects and cost; capabilities add **kinds**, what is special about an action, and **deliveries**, how it reaches what it affects. So one cooldown, cost, range and interruption rule serve a hero's ultimate, a rifle and a barracks, as the Gameplay Ability System makes an attack an ability and StarCraft II runs weapons and abilities through the same effects.
- **One schema for every action.** Every action is a table of a package; a field its kind does not use fails the load.
- **Slot kinds in the mode's order.** The mode declares where actions sit on a unit, and a unit's slots go kind after kind in that order, so a client binds its keys to the same slots on every unit.
- **A capability claims the slot kinds it fills.** A slot kind whose slots a capability fills of its own, as an inventory's are its items' and a use slot its object's, is a claim that the capability's module adds to a fact of actions, which refuses a claimed kind of more than one rank or of two capabilities, a unit type that puts actions in one, and more slots than a unit holds; the mode refuses a choice that fills one. Refused: each capability checking the sections of the others, which made items and interaction read each other's data.

## The pipeline

- **Each kind calls the shared steps.** `actions` gives one start step, the checks of the slot, its readiness and charges, the purse and the target, the clamp of a point into range and the reach, and one commit step, the spend of the cooldown, a charge and a toggle, beside `Payer` for the cost; each kind's own system calls them in its own order and at its own moment, as an ability's `ActivateAbility` calls `CommitAbility` in Unreal's Gameplay Ability System: a train and a build at their start, an attack at its strike, a cast as it resolves. Refused: a callback table that one core system walks for every kind, which takes the whole world for each candidate unit and changes the kinds' order within a tick.
- **A kind's block is the start's.** The start step tests the block of the action in its slot for every kind, `use` for a slot of the unit's use group and else `ActionKind::block` of its kind: an attack's `attack`, a cast's `cast`, a use's, a build's and a gather's `use`, a train's `produce`, so no kind tests its block by hand.
- **Stats is above the pipeline.** `actions` names no type of `stats`: an action's cost is paid through `units`' `PoolPurse` and `PoolPayer`, which `stats` implements for its pools; a target lives by `units`' `Life`, which `stats` writes; an action's names of stats and modifiers resolve through `units`' `StatNames` and `ModifierNames`, which `stats` gives; and a param is read by its place through the call frame's part that holds the action's params. Refused: `actions` above `stats`, which tied the pipeline of every game to one stat system; a contract crate between them, which held no module of its own; and a living test of `Dead` and the tags alone, which let a cast that resolves after the tick's damage and before its deaths hit a unit whose life the damage emptied.
- **All or nothing at delivery.** At delivery the checks run again, without the range; the cost, the cooldown and every effect the action queued apply together, or, when a check or the script fails, none of them, the failure recorded.
- **An interruption spends nothing.** A tag that blocks the action's group, or a new order, interrupts the windup back to its order with nothing spent.
- **A target an action does not take is ignored,** so a client may always send the unit under the cursor.

## Over time

- **Charges restore on a timer.** One comes back each `recharge_ms` while the count is below its maximum, the timer running from the tick it fell, as Dota 2's charge restore; a cooldown beside it is a lockout between two casts.
- **A range clamp or a walk.** A point beyond range moves in to the range with `clamp_to_range`, as League of Legends' Flash; any other aim beyond range walks the unit in range first, and a unit that does not walk drops the cast.
- **One rule for every toggle.** A cost the pools cannot pay turns a toggle off, and so do death and a tag that blocks casting, where Dota 2 lets each toggle choose; a second cast turns it off with no cost, so an empty pool can still turn one off.
- **A channel ends as Dota 2's and League of Legends' do.** A new order, death or a blocking tag interrupts it; with `damage_interrupts`, so does damage above 0 before shields absorb it, as Dota 2's channelled items end on damage; a heal does not. A channel's ticks know their target, so a held use can add to its object's progress.
- **A charged action resolves at release.** It charges in place of its windup and resolves when its order comes again or at its maximum; a move keeps it, as Varus walks while he draws.

## Cooldown cuts

- **A cut never ends a wait early.** A cut of a cooldown or a share of one rounds so that no ready tick is earlier than now, as Master Yi's takedowns cut his cooldowns.

## Kinds

| Kind | Capability | Group a tag blocks |
| --- | --- | --- |
| `cast` | `abilities` | `cast` |
| `attack` | `combat` | `attack` |
| `use` | `interaction`, run by `abilities` | `use` |
| `enter` | `interaction` | `use` |
| `train`, `build`, `gather` | `production` | `use` |
| `craft` | `items`, planned | `use` |

- **An item's action is a use.** An action in an item's slot is in the `use` group, whatever its kind ([Items](items.md)).

## Deliveries

| Delivery | Capability | Reaches |
| --- | --- | --- |
| `instant` | core | The target, at once |
| `ray`, `sweep` | `hitboxes`, planned | What a ray or a swept shape meets ([Hitboxes](hitboxes.md)) |
| `projectile` | `projectiles` | What a projectile unit meets |
| `area` | `areas` | The units inside an area unit |

- **Projectiles and areas are units.** They have stable ids, positions, tags, script state and vision, as a missile is a unit in StarCraft II's data editor, and no life, so no attack reaches one.
- **A homing projectile always catches its target.** The load requires it to fly faster than `max_move_speed`; a target that dies, becomes no target or teleports disjoints it.
- **A line projectile hits in path order.** It hits each unit its filter selects whose body comes within half its width of its path, by the path's nearest point to each, then stable id.
- **A falling projectile is a closed form.** With `gravity`, its point at each tick is computed from its distance flown in one step and rounded once, so no tick's rounding carries into the next, as a stepwise integration's would; on a planar map it rises and lands where its line ends, so its hits stay the ground plane's; on a spatial map it falls from its aim and sweeps each tick's chord, as Unreal's projectile movement does. Planned.
- **A projectile may fly a curve.** A projectile type's `curve` flies a cubic Bézier curve from its launch to its target's center, its inner points at fractions of the way and heights above the highest of its ends and the ground between, as Zero Hour's `calcFlightPath` sets them; its point count is ⌈length ÷ speed a tick⌉, the length `BezierSegment::getApproximateLength`'s, and each point is the curve's value in one step, rounded once, as OpenRA's `WPos.LerpQuadratic` computes a flight, so no rounding gathers. Its end follows a living target by `follow` a second, as `DumbProjectileBehavior::update` follows its victim; a curve takes no `homing`, `gravity`, `range`, `stop_on_hit` or `once_per_cast`, which only a line or a homing flight uses.
- **A curve meets units by its point.** After the tick's movement, a curve's sphere of half its width at its point meets the volumes of the living units its `hits` selects, its target's always and its source's never, the lowest stable id first, as Zero Hour's partition manager tests a shell's sphere after the frame's updates; it sweeps no path, so a shell passes a body between two points as it does in the original, where a line keeps its swept test.
- **A projectile hits by any of its filters.** `hits` takes `none`, a filter, or a list of filters, any one of which selects a hit, as 0 A.D.'s `MatchesClassList` selects by any entry, OpenRA's warheads by any of their `ValidTargets` and Zero Hour's `ProjectileCollidesWith` by any bit; one filter holds no "or" of two tags, and a list of none is refused, as `none` says it.
- **An area triggers once and holds while it lasts.** It reaches the living units its filter selects once after its delay, and holds its modifiers on the units inside while it lasts, as a Dota thinker carries an aura.
- **Every delivery records its hit.** How it reached a unit, its projectile or area, the aimed unit, where, how far and from which way, reaches `on_hit`, `on_end` and the damage it deals.
- **One record for every delivery.** A projectile's hit or end, an area's and a dash's end each join the tick's one queue of deliveries, with what the delivery carries: an action's, whose hooks and lists run as the deliveries end, or an attack's shot, whose damage combat queues as the hits land, so a new delivery reports in one shape. Refused: an attack's hits in a scratch of their own beside the queue, which split one report in two by its payload.

## Effects

- **Data and scripts queue the same effects.** An effect list queues in order as a script's calls do, before the hook of the same name, in one call, so a failed hook applies neither.
- **A purge spares what is held.** A purge ends the applications of modifiers that grant a tag, but not an instance a passive, an aura, an area or a player holds, as Dota 2's dispels never remove passives or auras.
- **A launched area chains in data.** An area an effect lands runs its own lists, and may launch again, so a chain in data is as deep as its nesting; a projectile has no launch effect, as data knows no aim for it.
- **Each capability registers its effects.** A listed effect is a type of the module that registers its data key, read as a trait object of `actions` that gives the core what it checks, its numbers, its duration, its modifier, the lists it holds and its own rules, and resolves into the module's own type, which queues the same effect value as the module's `ctx` call; so a mode of weapons alone runs them too, the reference lists each key under its module, and the load refuses a planned effect. As Bevy's type registry reads a scene's components and the Gameplay Ability System's effects are classes of their modules. Refused: closed enums of every capability's effects in `actions`, which named each capability; one enum of their data with each capability's resolve, which still named their data; and a resolved effect in each capability's book by index behind a table of callbacks, a second book and a lookup for what one call through the trait object does.

## Network

- **A client predicts the start, never the effects.** It predicts its own actions' start; their effects come from the server, and it flies no projectile and triggers no area.
