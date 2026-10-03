# Net

Design: [Modules](../design/02-engine-core.md#modules), `net`. Rules: [Issue log](../../AGENTS.md#issue-log).

## Decide

## Research

- Every replicated component of a unit goes to every client that sees the unit. Design 04 sends pools other than life to the owner or the team ([Combat](../design/04-capabilities/combat.md#network)), and experience and points to the owner ([Progression](../design/04-capabilities/progression.md#network)).

## Ready

- The client's lead is stated wrong. [Bevy](../design/02-engine-core.md#bevy) says Lightyear keeps the client's tick ahead "by the round trip, so its inputs land in time", and `SimClient::build` says the same of "the server's" tick. Lightyear 0.30.1 (`LocalTimelineSync::sync_objective`) keeps it ahead of the server's present tick by half the round trip, a jitter margin, a sync error margin and one tick. It is a full round trip ahead only of the newest server state the client holds.
