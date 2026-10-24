# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Walkers get through

The slice of [Walkers get through](docs/design/walkers-get-through.md), in navigation alone, a
batch each; its checks are that document's. It stands first, as a walker that stands for good
with a destination breaks every match that holds one.

### W3. A route in sight (navigation's planner)

- [ ] A route from a cell no open cell borders finds no way to a goal the walker sees
  (`route_planner/mod.rs:243`) → a goal in sight is a line, before the search
  (walkers-get-through.md, A route in sight is a line).

### W4. Friends that yield through friends (navigation's steering)

- [ ] A friend's yield search takes the friends near it as blockers (`steering/mod.rs:405`,
  `route_planner/mod.rs:543`) → it passes through the idle friends the walker may move, and each
  it passes or ends on steps aside in turn for its walk, 2 deep at most
  (walkers-get-through.md, A friend yields through the friends that yield).

## The driver keeps its signals

These items change the `drive` repository and wait for the driver's own work; then the drive table
requires the release that holds them.

- [ ] A command's first error is its log's first line → an `error` pattern per command in its
  table; the Zero Hour check's is `ERROR|panicked at|error\[`.
- [ ] A timeout reads as a FAIL → a TIMEOUT of its own in the verdict.
- [ ] A compile error of the test binaries shows as `FAIL index`, as the `index` step runs
  `test(=index)` → the build is a step of its own.

## Foundations
