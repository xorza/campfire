# Optimization: change-driven passes

The work of the next optimization session, which the user starts: the change-driven passes, the one part of the workspace review the user moved out of the hardening, and what the first session on 2026-10-15 measured. Each item is written in a session with its bench before and after, and an item that does not pay is dropped, as the user decided.

## The root cause: Passes scan where they should visit what changed

- **Mechanism.** Systems query every unit and test each one, and no shared structure holds "work due at tick t".
- **Structural change.** Event-driven passes use `Changed<…>` filters, and a full walk runs only when a book or the static set changes. Timed work goes through one `DueRun`, a run kept sorted by due tick, in `sim`. It serves the think pass, the charges, the toggles and any later timer. It is a cache that each restore rebuilds, outside the state hash. A change to the static bodies patches the rows and the regions it touches.
- **Guard.** Each item names its bench ([Benches](../docs/design/02-engine-core.md#benches)), which the session runs. No test bounds a pass's cost.

## Method

- **A bench decides.** Before an item, save a baseline of the cases it can move; after it, compare. `./drive.py bench -- '<regex>' --save-baseline <name>`, then `--baseline <name>`.
- **Pin an A/B run.** A desktop under load moves an unpinned case by 10 % or more. Build the bench binary before and after the change (`./drive.py bench --bench harness --no-run`, with the change stashed for the first), and run both alternately with `taskset -c 4 <binary> --bench --exact <id>`.
- **Profile first.** `RUSTFLAGS="-C force-frame-pointers=yes" ./drive.py bench --bench harness --no-run --target-dir target/frame-pointers`, then `taskset -c 2 perf record -e cpu_core/cycles/ -F 2000 --call-graph fp` on the binary with `--bench --exact <id> --profile-time 15`. The machine is hybrid: name the P-cores' event, or the report counts the E-cores' few samples.
- **The battle cases run the whole tick.** Each `integration/battle/crowded_<stage>` times one stage of a full 1,000-unit tick, so a profile of any one of them is the profile of that tick.

## What the first session measured

The baseline (`b11`, criterion's, in `source/target/criterion`) of the cases these items can move, unpinned: `server_tick/worst_3v3` 488 µs, `server_tick/mean_3v3` 447 ms, `battle/crowded_think` 83.9 ms, `crowded_act` 9.6 ms, `crowded_move` 6.3 ms, `crowded_resolve` 33.0 ms, `collision/boxes` 686 µs, `gather/two_fields` 25.9 µs, `pathing_grid/enter` 13.8 µs, `atomic/body_box/reach_box` 55.5 µs.

The share of each pass in a tick, inclusive, in the mean 3v3 tick and the 1,000-unit battle tick:

| Pass | Mean 3v3 | Battle |
| --- | --- | --- |
| `Thinking::think`'s scan of the thinkers, without the script calls | 0.2 % | 0.1 % |
| `Holds::want_passives` | 1.1 % | 0.5 % |
| `Toggles::run_toggles` | 0.4 % | 0.25 % |
| `Holds::hold_charges` | 0.3 % | 0.1 % |
| `HeldPass` | 3.7 %, of which `UnitsNear::current` 2.3 % | 0.2 % |
| `Routing::route_units` and `plan_routes` | 0.8 % | 0.2 % |
| `StaticTracking::track_static_bodies` | 0.2 % | 0.1 % |
| `Walking::keep_in_bounds` | 0.1 % | 0.1 % |
| `Flying::fly` | 0.4 % | 0.2 % |
| Steering's walking grid, built each tick | 1.5 % | 2.0 % |

- **A pass below about 0.5 % cannot show its gain** in a bench whose runs differ by 1 %.
- **`HeldPass`'s cost is the shared unit grid.** `UnitsNear::current` builds the grid on its first read in a tick, and the held pass reads first; another system would build it if the held pass did not.
- **The static bodies' item is in the code.** `BodyIndex::update` and `PathingGrid::update` take only the bodies that came and went, and label again only the regions they touch: `pathing_grid/enter` is 13.8 µs.
- **Steering's walking grid, built only for a stuck walker, does not pay.** Pinned A/B: `battle/crowded_move` 6.69 ms before and after; `server_stage/worst_3v3_move` 149 µs and 170 µs before, 149 µs and 156 µs after, within its noise. The change was dropped.

## What the second session did

The session on branch `perf-change-driven-passes`, on 2026-10-15, took every item; its journal, [PERF](PERF.md), holds each idea with its numbers and its verdict. Kept: each segment's seed derived once; `BodyBox::push_out` with one probe; `DueRun` in `sim`, which `hold_charges` uses; `run_toggles` over the set of units with a toggle on; `route_units` over the walkers that changed; the gather loop's relations read once for each read of its view. Dropped, as no case can show them or they cost more than they save: `DueRun` for the thinkers, `keep_in_bounds` over changed positions, the held pass's player modifiers, `want_passives` past its scan, `plan_routes`' list of waiting asks, the waiting workers by node, homing projectiles by target, `room_for` near the box, and one restore target for each replay.

## Items

Files: `network/quic/src/admission.rs`. Goldens: none move.

- [ ] `quic/src/admission.rs:18-84`: a map and an expiry queue. It waits for a bench of a flood of handshakes, and so for `criterion` as a dependency of `quic`, which the user decides.
