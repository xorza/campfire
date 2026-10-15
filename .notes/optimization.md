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

## Items

Files: `sim/sim/src/` (the new `DueRun`), `capabilities/navigation/src/**`, `capabilities/stats/src/held_pass.rs`, `capabilities/core/actions/src/holds.rs`, `capabilities/orders/src/thinking.rs`, `capabilities/production/src/gather_loop/mod.rs`, `capabilities/projectiles/src/flying.rs`, `capabilities/abilities/src/toggles.rs`, `session/{runner,protocol}/src/**`, `network/quic/src/admission.rs`, `capabilities/core/geometry/src/body_box/mod.rs`. Goldens: none move. A pass visits fewer units with the same result.

- [ ] `DueRun` in `sim`, a cache that each restore rebuilds. `orders/src/thinking.rs:74-93`, `actions/src/holds.rs:22-36` and `abilities/src/toggles.rs:20-64` use it.
- [ ] `navigation/src/routing.rs:325-403`, `route_asks.rs:127-152`: `Changed<Destination>`, every walker only on a static change, and a list of waiting asks.
- [ ] `navigation/src/walking.rs:320-327`, `navigation_column.rs:78-94`: `keep_in_bounds` over `Changed<Position>`, and `room_for` near the box.
- [ ] `stats/src/held_pass.rs:105-208`: re-derive the holds on a change of owner, tags or `PlayerModifiers`.
- [ ] `actions/src/holds.rs:45-106`: `want_passives` over `Changed` filters and `ReadyWaits`.
- [ ] `production/src/gather_loop/mod.rs:176-190,331`, `projectiles/src/flying.rs:26-35`: waiting workers by node, `Changed<Node>`, and homing projectiles by target.
- [ ] `protocol/src/server_seeds.rs:24-33`, `runner/src/session/mod.rs:107-117,217-224`: test `segment <= last` and derive the seed once, and one restore target for each replay.
- [ ] `quic/src/admission.rs:18-84`: a map and an expiry queue.
- [ ] `geometry/src/body_box/mod.rs:336-343,533`: pass the `Edges` and the probe already made.
