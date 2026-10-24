# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## The driver keeps its signals

These items change the `drive` repository and wait for the driver's own work; then the drive table
requires the release that holds them.

- [ ] A command's first error is its log's first line → an `error` pattern per command in its
  table; the Zero Hour check's is `ERROR|panicked at|error\[`.
- [ ] A timeout reads as a FAIL → a TIMEOUT of its own in the verdict.
- [ ] A compile error of the test binaries shows as `FAIL index`, as the `index` step runs
  `test(=index)` → the build is a step of its own.

## Foundations
