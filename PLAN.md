# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## The audit resolves a stack once

`measure` alone, a batch: the Windows gate passes.

### A1. One resolution for each stack (measure's audit)

- [ ] The allocation audit resolves the symbols of every trace of every frame
  (`alloc_trace/mod.rs:57`, `alloc_audit.rs:144`), and the backtrace crate resolves through
  `dbghelp` on Windows: each of the 10 tests of `campfire-net-stack`'s `alloc` binary traces up to
  3 allowed allocations in each of 256 frames, and takes 0.2 s on Linux and past 30 s on
  `windows-2025`, while its one test that allocates nothing passes there → the audit matches a
  trace by its frames' addresses first, and resolves the symbols of a stack it has not met, once,
  keeping which budget entries it passes. Builds: a pass visits what changed.
- [ ] `02-engine-core.md`'s allocation audits say that a stack resolves once, with the Windows
  times as the reason.
- Checks: a unit test of `measure` in which 256 frames of one stack resolve it once and match as
  before; the net-stack `alloc` binary passes on CI's `windows-2025`.

## Foundations
