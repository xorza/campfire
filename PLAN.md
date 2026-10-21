# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Slice: Zero Hour automatic attack

A Zero Hour player's idle units and base defenses attack the enemies that come within their weapons' reach, with no order ([proposal](docs/design/12-zero-hour-auto-attack.md)). Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. The idle scan (L)

`actions`, `orders`, the capabilities' tests and a bench case. First: the scan is generic and every later batch reads it.

- **The source of an attack target.** `ActionProgress`'s attack target holds its source, an order or the scan, and each caller of `set_attack_target` names it; `Destinations::chase` walks to an ordered target and drops a scanned one that no weapon of the unit reaches, with no walk. Builds: what can be forgotten is a type.
- **The `acquire` section.** A unit type's `acquire = { scan_ms, targets }`, `targets` a list of filters of which any selects, as a projectile's `hits` is, read and checked at the load: a period above 0, one filter at least, a type with a weapon slot and no build or gather slot, each refusal an error of its own. Builds: a name is resolved once, at the load.
- **The scan.** A part of each unit of a type that scans, its next scan's tick, staggered by stable id as `NextThink` is, in an index of its own; in Think, on a role that computes state, each unit due that stands idle takes the nearest target its team sees that a filter of `targets` selects and that a weapon of it selects and reaches, by the unit grid, the lower stable id at an equal distance, and is due a period later. Builds: a pass visits what changed.
- **Check:** a test of the first scan's tick of two units of hand-computed stable ids; of the pick among targets at hand-computed distances, a tie to the lower id, and each target the scan passes over: one no filter selects, one no weapon reaches, one the team does not see, and a neutral under `hostiles`; of a scanned target that walks out of reach, dropped with no chase, against an ordered one, chased; of a unit that walks, which does not scan, and of a client's match, which never scans; of each load refusal. The bench case `integration/acquire/idle` scans 512 idle armed units at 250 ms and runs once in the gate. A state hash's pinned values move once, for the new part of a unit's progress.

### 2. Zero Hour's scan (M)

`import`, the rules package, 09. After batch 1, whose section it writes.

- **The neutral team.** The imported mode's relations make each player's team regard `neutral` as `Neutral`; 09's decision on the gather drops its refusal of that relation.
- **The base defense's tag.** `base_defense`, of `FS_BASE_DEFENSE`, in the rules package's tags and the import's tag list.
- **The scan of each type.** A type of an imported weapon whose AI module's `AutoAcquireEnemiesWhenIdle` holds `Yes` gets `acquire`, of its `MoodAttackCheckRate` or 2 s, as the import converts a time; `ATTACK_BUILDINGS` writes `targets = ["hostiles"]`; the rules package's `[defaults.acquire]` gives `targets = ["hostiles:!structure", "hostiles:base_defense"]`.
- **Check:** fixture tests of a type of `Yes` and a rate, of `Yes` and no rate, of `No`, of `ATTACK_BUILDINGS`, of an unarmed type of `Yes`, which gets none, and of the relations the mode writes; `cargo drive run zero-hour`, on request, moves its fingerprints and its skirmish play, whose idle units now fire.

### 3. Base defenses in the check (M)

`zero-hour-check`, its golden, 09, 04-capabilities/control.md. Last: it plays what the two batches before it give, and ends the slice.

- **A defense play.** China on Tournament Desert builds a power plant and a Gattling Cannon, and a bot's America dozer drives past the cannon; the play pins the tick of the cannon's first hit and of the dozer's death, with no order to the cannon.
- **The skirmish play.** Its golden pins the first hit on one of player 1's crusaders that no order of the bot gave.
- **The fold.** The orders decisions go to control.md, in place of the planned target acquisition, the import's and the losses to 09; the proposal is deleted.
- **Check:** `cargo drive run zero-hour` passes with its golden blessed, each moved value named by its cause; the user plays a skirmish in the client and watches a Gattling Cannon and idle tanks fire.
