# Stats

## Mechanism

- **Unreal's Gameplay Ability System, where the field agrees.** A base value from type and level, a current value modifiers change through one formula, stacks counted by source, and tags that block actions and grant immunity; movement and pools follow League of Legends, where its rules are exact.

## Stats

- **One formula, order-free.** `value = (base + Σ add) × (1 + Σ pct) × (1 − max cut)`, clamped to the stat's limits: `add` and `pct` sum, so modifier order never matters, and only the largest `cut` counts, as only the strongest slow does in League of Legends; each product rounds once.
- **The engine reads only the stats its mechanisms need,** bound by the mode's names; a stat the mode does not declare keeps its type's value.
- **Regen carries its remainder,** so a second gains exactly the regen.

## Pools

- **At most eight pools,** so a unit's pools are fixed arrays a rollback copies without allocating.
- **A rising maximum raises the amount by as much, and a falling one keeps it.** League of Legends' rule, where Dota 2 keeps the fraction, which needs a rounding the engine does not take.

## Modifiers

- **An interval's policy at a respawn.** A dead carrier's intervals run no hook, and keep their clock; as the carrier comes back, the modifier's `interval_revival` says what its interval does: `continue`, the default, comes when it would have, `reset` comes a whole interval later, and `fire_and_reset` comes at once, then a whole interval on, the three policies of Unreal's `EGameplayEffectPeriodInhibitionRemovedPolicy`. An interval never comes twice in one tick, so a clock that fell behind does not burst.
- **One instance per id and source.** A carrier holds at most one instance of an id from each source; a reapply refreshes, stacks or is ignored, as the modifier says.
- **A duration counts whole ticks from the end of its tick,** so it holds the same at any rate to within one tick.
- **An instance lives while something keeps it.** A passive's rank, an aura, an area, a player and an application each keep it; it ends only when nothing does.
- **An instance ends with its last stack.** An instance left with no stack ends, with its clock, its hooks and intervals, whether its stacks ran out or a script wrote `m.stacks = 0`, so the two ways to none end alike; a passive stays with none, as its rank keeps it. Refused: an instance of no stack that `has_modifier` reported and whose intervals ran on.
- **Passives by one pass.** Each source pushes what its units want into one list, and one pass applies it and releases the rest, visiting only the units whose source or modifiers changed.
- **Stats builds what the actions read of it.** The param book of every action and modifier, the modifier effects, `modifier`, `purge` and `restore`, the ways each modifier is applied and the grants of the actions' passives and holds are stats', as it sits above `actions`; it builds each from the actions' data once their load checked it, and pushes the passives' grants right before each grant pass, ordered against the actions' sets ([Actions](actions.md#mechanism)).
- **Auras with no linger.** Every unit within an aura's radius holds its modifier, and loses it the tick it leaves; the sim has no flicker to hide.
- **An area holds as an aura does.** An area's `inside` projects each of its modifiers, on its source, on the source's other allies or on the others that may be attacked, and the one pass that holds auras selects both, by the filter and the living test, as a Dota thinker carries an aura. Refused: a walk in each capability, which disagreed on a unit of no life and on the tags a filter selects only when it names them.
- **Player modifiers are upgrades.** A player's modifier holds on each living unit it selects, so an RTS's upgrades and a guild's bonuses are player modifiers.
- **A magnitude is captured at application,** as the Gameplay Ability System captures one: a later rank changes the next application, not one held. A scaling stat change follows its source's stats while held, as League of Legends' bonus-scaling passives do, so the load refuses a loop between such changes.
- **An application never fails after its call.** The load checks that every way a modifier is applied gives each param it reads, so no application fails later.
- **A unit type takes modifiers by upgrades.** A type's `upgrades` lists entries of `triggers`, modifier ids any one of which, or with `all` every one, its player's modifiers and its own must hold, `conflicts` none of which they may hold, and the `modifier` the unit takes while they are met, held by a hold of its own (An instance lives while something keeps it), as Zero Hour's `UpgradeMux` tests `TriggeredBy`, `RequiresAllTriggers` and `ConflictsWith` and OpenRA's `GrantConditionOnPrerequisite` grants a condition. A unit tests its entries as it spawns, as its site completes, and as it or its player gains or loses a modifier, and the test visits only the units of the types that name that modifier. Refused: a player modifier for each type an upgrade changes, whose filter would need a tag for each of Zero Hour's 643 types of its factions, past the 256 a match holds; the effect written into the research, which differs by type.

## Tags

- **A tag has properties only when the mode gives them:** it blocks moving, an action group, being a target or taking damage, hides or detects, or makes the unit immune to modifiers that grant other tags.
- **One question each.** Each system asks only its own question of the tags, and none reads modifiers for a tag.
- **Stopped, not dropped.** A block keeps the order behind it, and the action runs again when it ends, as League of Legends buffers input; an interrupted windup starts again from nothing, as Dota 2's cast point.
- **Immunity suppresses, not refuses,** as tags are derived: a suppressed modifier takes effect again when the immunity ends, and no order of modifiers changes the result.
- **The engine's tags hold the first places,** each declared by its module, so they are numbered the same in every match.
- **Airborne is a tag of height.** A flier holds the engine's tag `air_target` while its height above the ground passes its airborne height, granted and taken as it crosses it, as Zero Hour marks `OBJECT_STATUS_AIRBORNE_TARGET` and OpenRA grants its `AirborneCondition`; a weapon picks air or ground targets by it; its name says what it serves, as `airborne` is a game's own word, the test MOBA's knock-up. Refused: the tag of the type's layer, fixed per type, under which a helicopter on its pad is immune to ground weapons.

## Levels

- **A level is state; stats follow it at once.** `progression` changes it.

## State and derived

- **Stats and tags are derived, never state.** They are computed again whenever a unit's modifiers, level or type change, and after a restore, before any system reads them; the parts that hold a stat's effect, a step and the pools' maxima, are state.
- **Stats' rules as data, planned.** With the slice that needs each: `Mul` and `Set` modifiers with a priority, hooks and intervals that stats owns, and a unit's pools as `Option<Meter>` with a niche in place of a fixed array of 256 bytes.

## Network

- **A client derives what it predicts.** It keeps its own units' modifiers as the server sends them and derives their stats, tags and step as the server does, so its predicted movement obeys them; it creates no modifier but an aura's, so a stun learned late costs a correction.
