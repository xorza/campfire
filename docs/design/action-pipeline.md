# One action pipeline: proposal

A slice proposal for plan batch 5.3, for review before it is built. When the batch lands, each lasting decision moves into [actions](04-capabilities/actions.md), [stats](04-capabilities/stats.md) and [Structural rules](02-engine-core.md#structural-rules), and this document goes.

**The slice.** Attacks, casts, uses, trains, builds and gathers start and go off through one pair of steps in `actions`, and `actions` no longer depends on `stats`. A player sees no change: each kind starts, pays and cools down at the tick it does today.

## Decisions

- **Start and commit, as the Gameplay Ability System.** `actions` owns two steps: a start, which checks the slot, its readiness and charges, the purse, the target, the kind's block and the reach, and begins the action's progress; and a commit, which pays the cost in pools and player resources and spends the cooldown or a charge. Each kind calls the commit at its own moment, as an ability calls `CommitAbility` where its design wants it: a train and a build at the start, an attack at its strike, a cast at its resolve, a gather never. Refused: one commit moment for every kind, which moves when trains and builds pay and every golden with a train or a build; and a commit inside each kind's own loop, which is the four copies of today (K-17).
- **Each kind calls the shared steps.** `actions` gives one start step, the block test, the check, the clamp and the reach, and one commit step; each kind's own system calls them in its own order, as an ability's `ActivateAbility` calls `CommitAbility` in GAS. Refused: a callback table that one core system walks for every kind, as `Deliverers` keys the deliveries, which takes the whole world for each candidate unit in Act and changes the kinds' order within a tick; and a closed match in the core, which names the kinds' capabilities.
- **A kind's block is `ActionKind::block`.** The start tests it for every kind, an attack's `Attack`, a cast's `Cast`, a use's `Use`, a train's, a build's and a gather's `Produce`, so no kind tests its block by hand.
- **Stats sits above actions.** `actions` asks for what `stats` gives through contracts in `units`, which `stats` fills, so the layer order is `units`, `actions`, `stats`, then the capabilities above both. Refused: `actions` above `stats`, as today, which ties the pipeline of every game to one stat system (C-3, E-5); and a contract crate of its own between them, a crate with no module that only re-states the four contracts below.
- **The purse is a contract.** `units` holds the cost types an action names, a pool's id and a cost of pools, and a `Purse` with `affords` and `pay`, which `stats` installs for the units with pools and `players` extends with a player's resources. An action of a mode without stats costs no pool, and the load refuses a pool cost there.
- **The living test is a contract, and `Life` is units'.** A target lives when it is not `Dead`, its life is not empty and no tag property makes it untargetable, which `units` answers from its own parts: `Life` and the `Meter` it is made of move to `units`, and `stats` keeps the pool book, the maxima and the regen, which write the meter. Refused: `Dead` and the tags alone, which let a cast that resolves after the tick's damage and before its deaths hit a unit whose life the damage emptied; and the deaths moved before the resolve, which moves the grants at resolve and the death hooks.
- **Params and holds are hooks of stats.** A param that scales by a stat, a passive and a toggle's or a channel's hold are what `stats` registers on the action pipeline's hooks, as a capability registers a kind; the pipeline reads a param's value through the frame, which stats fills as a call starts.
- **The effects of stats move to stats.** `modifier`, `purge` and `restore` queue a `StatsEffect`, so each becomes a listed effect of `stats`, which registers it by its key as any capability does (04-capabilities/actions.md, Each capability registers its effects); the core keeps `spawn`. The modifier ways and their appliers, which 4.4 put in `actions`, move to `stats` with them.

## Goldens

- **No golden moves by design.** Each kind keeps its commit moment, and the core keeps today's order of the steps within a tick. A golden that moves shows a step whose order changed, and the batch explains each such move before it lands.

## Checks

- A test plays each kind through the one start and its commit, and pins the tick it pays and cools down at, against the same match before the batch.
- The release test of needs and uses covers the core modules, and `actions` lists no dependency on `stats`.
- A mode without stats loads actions that cost no pool, and refuses one that costs a pool.
- `./drive.py check` passes, and no golden moves.
