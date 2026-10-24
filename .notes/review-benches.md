# Bench audit

The inventory on 2026-10-24: 18 bench targets, 31 `bench.rs` files, 111 calls that register a case, some of them in loops over sizes or stages. No case was run.

## Proposal

### What the inventory has

- **Atomic, well covered:** `math` (num, vec3, direction, root, rounding, rng), `geometry` (grid, body_box, body_grid, placed), `net-bytes` (varint), `net-channels` (fragment reassembly), `net-emulator` (admit), `net-udp` (admission, endpoint), `protocol` (input chain, server seeds), `navigation` (body_index), `script` (script host calls).
- **Integration, well covered:** the MOBA 3v3 server tick, worst and mean, and each stage of it (`harness`); a 1,000-creep battle by stage (`harness`); the match build and the package load (`harness`); the client and server frames and the rollback scan (`net-harness`); collision, the pathing grid's update and the route planner (`navigation`); fog (`vision`); gather, acquire and projectiles (`capabilities`); the state hash, snapshot and delta (`sim`); replication and messages (`net-replication`, `net-messages`); the session log (`protocol`); unit picking (`client`); the allocations of the hot scripts (`harness`, `allocations`).

### What it lacks

- **The RTS workload.** Every integration scene is the MOBA 3v3, a synthetic kernel scene or a capability scene. No case runs a Zero Hour match: hundreds of units of two bases that build, train, gather and fight on an imported map. That is the target workload of the engine, and it is the workload where the navigation changes of this week (yields, crowd walls, route asks) cost the most.
- **Bots.** `session/bots` thinks for every bot each think period, a script call per bot, and has no case.
- **Client per-frame work.** Only unit picking has a case. The interpolation of the units that the client holds (`net-interpolation`, `net-frame-interpolation`) and the client's fog upload run each frame and have no case.
- **Codec and packets.** `codec` (snapshot and payload encoding) and `net-packets` (packet build and parse) are only inside `net-replication` and `state_delta`.

### What is inconsistent

B1 gave every bench one scheme of names, input sizes and throughput (02-engine-core.md, One scheme of case names). Left:

- **Mean and worst.** The design says that an end has a mean case and a worst case. `server_tick` and `server_stage` have both. `server_frame` and `client_frame` have `walk` and `worst_1v1`, with no mean of the 1v1 and no worst of the walk. The capability scenes and navigation's scenes have only the mean. Target: every integration case of a tick that changes over a match has `mean_<scene>` and `worst_<scene>`. A steady-state scene (gather, projectiles) keeps one case, and its doc says that it is steady.
- **The capability benches' home.** `capabilities/src/bench.rs` holds gather (production), acquire (combat) and projectiles (projectiles) in the top crate, as only the top crate has the match. That is correct for integration cases. Each owner crate also gets its atomic cases.
