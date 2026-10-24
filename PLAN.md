# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Benches cover the hot paths

The audit in `.notes/review-benches.md`: every hot path has a case on a workload of its real size,
and
every case follows one scheme of names, sizes and throughput. Each batch ends with `cargo drive
gate`,
which builds every bench; no batch runs one.

### B2. Navigation's tick (`navigation`)

- [ ] `integration/walking/{mean,worst}_<scene>`: the Move stage of navigation (`Steering::read`,
  `CrowdCosts::update`, `Walking::steer`, the yield search) on `KernelScene::UNITS` walkers in three
  scenes: `open`, walkers that cross an open field; `crowd`, walkers through an idle crowd of their
  team; `corridor`, a column of idle friends in walled lanes, as in the dozer case.
- [ ] `integration/route_asks/<scene>`: the routes that a wave and a group of stuck walkers ask,
  with crowd prices and with walls.
- [ ] Atomics: `atomic/route_planner/{yield_place, nearest_open, line}`, `atomic/steering/steer`,
  `atomic/crowd_costs/update_<n>_changes`.

### B3. Capability atomics (`combat`, `stats`, `areas`, `orders`, `abilities`)

- [ ] One atomic group in each owner crate, on `INPUTS` made inputs: combat's damage weigh, stats'
  recompute of a unit's modifiers, areas' trigger test, orders' chase step, abilities' cast check.

### B4. The RTS workload (`zero-hour-check`)

- [ ] `integration/rts_tick/{mean,worst}_<play>` and `integration/rts_stage/...`: a tick of the Zero
  Hour check's plays on the imported package, two bases that build, train, gather and fight. The
  cases skip when the package is missing, so no fresh clone and no CI runs them.
- [ ] `integration/bots/think_<n>_bots`: a think period of those plays' bots.

### B5. Client, codec and frames (`client`, `net-interpolation`, `codec`, `net-packets`, `net-harness`)

- [ ] `integration/client_frame/interpolate_<n>_units`: the interpolation of the units that
  the client holds, on 1,000 units.
- [ ] `atomic/codec/{encode,decode}` on a snapshot of `KernelScene::UNITS` units, and
  `atomic/packets/{build,parse}`.
- [ ] `integration/server_frame` and `integration/client_frame`: a `mean_<scene>` and a `worst_<scene>`
  case of each scene, where now `walk` has no worst and the 1v1 no mean.

## Foundations
