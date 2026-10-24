# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## B5. Client, codec and frames (`client`, `net-interpolation`, `codec`, `net-packets`, `net-harness`)

- [ ] `integration/client_frame/interpolate_<n>_units`: the interpolation of the units that
  the client holds, on 1,000 units.
- [ ] `atomic/codec/{encode,decode}` on a snapshot of `KernelScene::UNITS` units, and
  `atomic/packets/{build,parse}`.
- [ ] `integration/server_frame` and `integration/client_frame`: a `mean_<scene>` and a `worst_<scene>`
  case of each scene, where now `walk` has no worst and the 1v1 no mean.

## Foundations
