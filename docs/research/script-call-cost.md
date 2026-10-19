# Script call cost

What the 3v3's load, match build and hot script calls cost, in time and in allocations, measured 2026-10-18.

## Machine and method

- **Machine:** Intel Core i9-13980HX, 32 threads, Linux 7.2, a laptop on mains power, with CI running remotely and no other build.
- **A run:** `./drive.py bench -- --exact <id>` for each case, criterion's defaults, at `3d0368653`. The allocation cases run in their own bench binary, whose global allocator counts each allocation, a growth among them, and report a call's count.

## Load and build

| Case | Time | What it holds |
| --- | ---: | --- |
| `integration/package_load/3v3` | 3.62 ms | The packages read from their files, parsed, checked, and their books built at the fastest rate |
| `integration/match_build/3v3` | 3.59 ms | A match of them built into a world, its books at the session's rate |
| `integration/script_compile/3v3` | 0.743 ms | The compile of every script of the packages into a host with the API bound |
| `integration/script_bind/3v3` | 0.132 ms | The bind of the whole script API into a new host |

- A match build runs once a session, and once a checkpoint in a verifier; no tick runs one.
- Of a build's 3.59 ms, the compile is 21 % and the bind 3.7 %; the books and the world are the rest, about 2.7 ms.

## Calls

| Case | Time a call | What it holds |
| --- | ---: | --- |
| `atomic/script/call` | 42.3 ns | An empty hook of two arguments |
| `atomic/script/handle` | 47.8 ns | The same, given a handle of a unit's size that shares an `Rc` |
| `atomic/script/native` | 227 ns | A hook that makes one call to a registered function |
| `atomic/script/native_handle` | 229 ns | A hook whose one call returns a handle |

- A handle argument costs 5.5 ns a call, its box and its reference count together; a handle a call returns, 1.6 ns.
- A call to a registered function costs about 185 ns, the most of each case.

## Allocations

| Case | Allocations a call | What it holds |
| --- | ---: | --- |
| `integration/script_allocations/find` | 181 | `ctx.find` of the enemies within 8 m of a unit of the crowded 3v3, its call started |
| `integration/script_allocations/on_think` | 72 | A creep's `on_think`, its call started |
| `integration/script_allocations/calc_damage` | 85 | The mode's `calc_damage` of a damage of 40, its pure call started |

- Each count is the same in every sample: the calls allocate the same each time.
