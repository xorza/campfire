# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Benches cover the hot paths

The audit in `.notes/review-benches.md`: every hot path has a case on a workload of its real
size, and every case follows one scheme of names, sizes and throughput. Each batch ends with
`cargo drive gate`, which builds every bench; no batch runs one.

### B5. Client, codec and frames (`client`, `net-interpolation`, `codec`, `net-packets`, `net-harness`)

- [ ] `integration/client_frame/interpolate_<n>_units`: the interpolation of the units that
  the client holds, on 1,000 units.
- [ ] `atomic/codec/{encode,decode}` on a snapshot of `KernelScene::UNITS` units, and
  `atomic/packets/{build,parse}`.
- [ ] `integration/server_frame` and `integration/client_frame`: a `mean_<scene>` and a `worst_<scene>`
  case of each scene, where now `walk` has no worst and the 1v1 no mean.

## Foundations
