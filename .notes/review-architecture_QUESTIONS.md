# Questions of the architecture plan

Each question waits for the user's decision. The plan's items point here.

## Q14. Batch 5.3: the living test leaves a gap in the tick

- **Item:** 5.3, the contract "a target lives when it is not `Dead` and no tag property makes it untargetable", which `docs/design/action-pipeline.md` decides, with "`Life` stays stats', and a unit with an empty life is `Dead` by the tick its damage resolves, as today".
- **Why it waits:** inside the tick, the order in `SimSet::Resolve` is `CombatSet::Damage`, then `ActionsSet::Resolve`, `HoldAtResolve` and `StatsSet::GrantAtResolve`, then `CombatSet::Die`, which inserts `Dead` (`combat/src/lib.rs:137-139`, `stats/src/lib.rs:248-250`, `actions/src/lib.rs:279-282`). A cast that resolves in that gap today finds no target in a unit whose life the damage emptied, as `stats::lives` reads the empty life. With the proposal's test it finds one. So the proposal's "as today" is true for the tick, not inside it.
- **Options:**
  1. **`Life` moves to `units` (recommended).** The life meter is the contract part: `units` holds `Life` and the `Meter` it is made of, and the living test reads `Dead`, the empty life and the tags. `stats` keeps the pool book, the maxima and the regen, and writes the meter as now. Cost: `Life` and `Meter` change crates, about 40 files' imports; no behavior changes, and no golden moves.
  2. **Deaths before the resolve.** `CombatSet::Die` moves right after `CombatSet::Damage`, so a unit at 0 life is `Dead` before any cast resolves, and the test reads `Dead` and the tags alone. Cost: the grants at resolve then apply after the deaths, and the mode's death hooks run before the casts resolve; goldens may move.
  3. **Accept the gap.** The test reads `Dead` and the tags alone. A cast that resolves in the tick a target's life empties hits it. Cost: a change of behavior; goldens may move.
- **Blocked:** the living test of 5.3. The rest of 5.3 goes on.

## Q15. Batch 5.3, K-17: a callback table or shared steps

- **Item:** 5.3, "One start and go-off pair in `actions` owns the checks and the times. Kinds plug in by a callback table" (K-17), as `docs/design/action-pipeline.md` decides.
- **Why it waits:** the code shares more than K-17 found. Attacks and casts already pass one check, `SlotTimes::checked_action` with `ActionCheck` and `Purse`, then `CheckedAction::in_range`, and spend through `SlotTimes::spend` and `Payer::pay` (`combat/src/attacks.rs:239-256`, `abilities/src/casts.rs:168-200`). What differs is each kind's own work: a turret's bearing, a cast's walk into range, a toggle's stop, the queue of a train. A table in the shape of `Deliverers`, a `fn(&mut World, …)` for each kind, takes the whole world for each unit and each tick in Act, a hot path where the kinds now run as ordinary systems over their own queries. In Unreal's GAS, the ability calls `CommitAbility` at its own moment; no central loop calls the abilities.
- **Options:**
  1. **Shared steps that each kind calls (recommended).** `actions` gains one start step, the block test, the check, the clamp and the reach, which gives a `CheckedAction`, and one commit step, the pay in pools and resources and the spend of the cooldown, a charge and a toggle. Each kind's system calls both at its own moment, in its own order. Cost: the proposal's table is dropped, and its decision is rewritten; no golden moves.
  2. **The callback table, as proposed.** One core system visits the units by stable id and calls each kind's start and go-off through the table. Cost: exclusive world access for each candidate unit in Act, and the kinds' order within a tick changes, so goldens may move against the proposal's "no golden moves".
  3. **Defer K-17.** Keep the loops as they are until a measurement or a new kind shows the cost of the duplication. Cost: K-17 stays open.
- **Blocked:** the pipeline bullet of 5.3, and 5.1, which needs the kinds' plug-in.

## Q16. Batch 6.4, N-18: a relationship from a slot's entity to its link

- **Item:** 6.4, N-18, the second half: "On the server, a Bevy relationship links a slot's entity and its link". The first half, one `Control` shape, is done.
- **Why it waits:** a one-to-one Bevy relationship (`PlayerLink` on the link pointing at the slot's `Player` entity, `SeatedLink` on that entity) would replace `Seats`' link, and Bevy removes an older link's `PlayerLink` as a newer one is inserted. But the door now runs as an ordinary system (N-4), and seats players through `Commands`, which apply at the frame's sync point. Two joins of one main key in one frame then miss each other: the second finds no `SeatedLink` yet, does not dismiss the first link, and the relationship drops the first's `PlayerLink` without a word, so the first link stays open with no seat. `Seats` holds the link at once, so today the first link is dismissed.
- **Options:**
  1. **Keep `Seats` as the one record of the link (recommended).** `PlayerLink` keeps the slot and team the link carries, which the hot input path reads, and `Seats` the link of each slot; a test pins that a second login in the same frame dismisses the first. Cost: two records of one fact stay, kept by the door's one seat and leave paths.
  2. **The relationship, with a frame's seats in a local.** `PlayerLink` becomes the relationship; the door keeps the links it seated this frame in a `Local`, which a second join reads before the commands apply. Cost: the local is a third record for one frame, and every path that seats must read it.
  3. **The relationship, the door exclusive again.** The door seats through the world at once, so the relationship holds at each join. Cost: N-4 goes back for the door.
- **Blocked:** the second half of N-18 alone.
