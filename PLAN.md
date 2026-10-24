# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Yields that end

A friend that steps aside reaches its place or is firm, never asked again and again; a batch, in
navigation alone.

### Y1. A yield's walk agrees with its search (navigation's steering and walking)

- [ ] A friend's yield search passes through the idle friends its walk may move, but its walk's
  steering takes them as blockers (`steering/mod.rs:410`), so a place on one is out of its
  detour's reach: its detour ends short in its own cell, it arrives short at once, and the walker
  asks it again every 4 ticks, as unit #366 of `integration/navigation/*_crowd` does from tick
  1,283 → a walk deeper than the walker's own (`AsideDepth` past 0) steers through the idle
  friends it may move, as its search does, and they step aside in turn
  (04-capabilities/navigation.md, A friend yields through the friends that yield). Builds: one
  word, one meaning, an idle friend the same to a yield's search and to its walk.
- [ ] A yield whose walk still ends short, as one a static body or a unit of another team holds,
  leaves its friend idle and not firm, so the walker asks it again (`route.rs:433` reports the
  end) → a walk deeper than the walker's own that ends short marks its unit hemmed in, as a search
  that finds no place does (04-capabilities/navigation.md, A friend that cannot yield is firm,
  and a stuck walker's route walls it). Builds: what can be forgotten is a type, the end of a
  yield one rule.
- [ ] `navigation.md`'s "A friend yields through the friends that yield" says that the friend's
  walk steers through them too, and "A friend that cannot yield is firm, and a stuck walker's
  route walls it" that a yield whose walk ends short is no place found, with the crowd scene's
  loop as the refused case.
- Checks: a walker behind an idle friend whose place lies on a second idle friend passes, the
  second stepping aside; a friend whose place a static body makes unreachable is hemmed in once,
  not asked again; the crowd scene's units all arrive, with no unit asked to step aside more than
  once for one walker.

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
