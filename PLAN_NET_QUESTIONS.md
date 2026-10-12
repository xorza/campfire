# Questions — network plan

The decisions the work on [PLAN_NET.md](PLAN_NET.md) met that are the user's to take: each names its item, the options, a recommendation, and what waits on it.

## A6. What a predicting client does with the state its owner does not hold

**The problem.** Design 14's D12 asks that a predicting client's schedule read only what its client holds: what it derives, a kind whose default audience holds every client that holds the unit, or, on the units it predicts, every kind but `server`. Its test, `release::tests::prediction_reads`, walks the schedule a client builds with every capability, and finds 39 reads that break it, listed in its `AWAITING`:

- **`server` types on the units a client predicts or holds**, which no client receives: `OnDeath` (`Dying::die`, `despawn_dead`), `RecentAttackers` (`Dying::respawn`), `StatusTags` (`Refresh::run`, `Construction::progress_sites`, `GatherLoop::tag_gatherers`), `PathWalker` and `OnPath` (`Routing::route_units`, `Destinations::follow_paths`, `OrderInputs::apply_player_orders`), `Builder`, `Gatherer`, `Site` and `Node` (the orders' and production's systems), `SeenBy` and `Sight` (`OrderInputs::check_player_orders`, `ProductionOrders`, `Construction::check_builds`, `Seeing::see`), `FlightState` (`Flying::fly`) and `AreaTrigger` (`AreaLife::trigger`).
- **`PlayerResources` on players' entities** (`Casts`, `Attacks`, `ProductionOrders`, `Construction`, `GatherLoop`, `Trains`), of the kind `resources`, which only the owner receives: a client holds its own player's, which these systems read through the owner of a unit it predicts, and no other player's.

On a client each such system finds the type missing: a required read matches no entity, and an optional one reads as absent, where the server's does not. Where a client's own unit holds the type on the server, its prediction differs: a scripted status, a death's handling, a build, a gather, a walk along a path.

**Options.**

1. **Authority-only systems.** A set that a predicting client's build leaves out holds the systems that compute what the server alone holds: vision's `see`, supply's and out of play's refresh, a dead unit's despawn, a node's end, a delivery's flight and trigger. The types they read stay `server`. What those systems do reaches the client as the server's state, a correction where its own unit changes. Cost: a client predicts no death, build, gather or status of its own units.
2. **Owner's kinds.** Each type a predicted unit holds and its owner's prediction must read to act as the server does joins `prediction`, which only the owner receives: `OnDeath`, `StatusTags`, `Builder`, `Gatherer`, `Site`, `PathWalker`, `OnPath`. Cost: more state on the owner's link, and each capability's design 04 file changes its network section.
3. **Both, by type (recommended).** Option 2 for the types a client's own unit holds and its prediction must read: `OnDeath`, `StatusTags`, `Builder`, `Gatherer`, `Site`, `PathWalker`, `OnPath`. Option 1 for the systems that compute the server's own state, or act only on what no client predicts: `Seeing::see`, `PlayerSupply::refresh`, `OutOfPlay::refresh`, `Dying::despawn_dead`, `Dying::respawn`'s attackers, `GatherLoop::despawn_empty_nodes`, `Flying::fly`, `AreaLife::trigger`. `SeenBy` stays the server's: an order's check that its target is seen holds on a client, which holds only what its team sees, but for a player out of play with `out = "spectators"`, which needs its own decision. The players' entities: D12's test takes a read of a player's entity through the owner of a unit the client predicts as the owner's, which needs no change of code, only of the test and of D12.

**What waits.** The rest of A6: the reads in `AWAITING`, which the test then empties, and A6's leaving the plan.
