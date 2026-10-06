# Plan questions

## The bench step's time in CI (Benches; design 13, Cost in CI)

**Item.** Design 13 estimated that CI's `Benches` step would run its cases in about 17 s, under the 30 s a suite may take. Run 37518789388 measured more: after the build of 4 to 12 s, the cases run 30.2 s on Ubuntu, 31.8 s on macOS and 34.2 s on Windows. About 20 s of that is the eleven cases that each play a whole 3v3 match of 6,000 ticks in the dev profile: `server_tick/worst_3v3`, `server_tick/worst_3v3_checkpointed` and the nine `server_stage/worst_3v3_<stage>`.

**Options.**

1. **Keep every case, and give the bench step its own limit of 45 s.** U13 already holds that a case runs as it measures, so a bench step is no test suite. Nothing is lost; CI takes about 30 s more on each platform than before the plan.
2. **Drop the nine `server_stage/worst_3v3_<stage>` cases, and keep the nine means.** The step falls to about 14 s. Which stage makes the worst tick is then found only by a profile, not by a case.
3. **Play the worst-stage cases over a shorter match.** A match long enough to reach the first fights of the waves, about 3,000 ticks, halves their time. The worst tick of a whole match can fall after it, so the cases would no longer measure what their ids say.

**Recommendation.** Option 1. The worst tick is the budget (design 04, Tick rate), and a stage's worst is what tells a change that moves it. 30 s more of CI on each platform is the price, and it falls on the platforms in parallel.

**Blocked.** No step: the cases are in, and the design's "Cost in CI" section states the measured times and points here until the choice is made.

## Kernels for the new hot stages (B7 to B9)

**Item.** B6's record found three stages past 10 % of a tick with no kernel: Inputs, 1.66 ms of the 3.13 ms worst 3v3 tick; Move, 0.90 ms of it; and Resolve, 13 % of the mean tick ([Record](docs/design/13-benches.md#record)). B6 adds a kernel step for each, B7 to B9.

**Options.**

1. **Implement them now**, as the loop takes plan items.
2. **Investigate first, and propose each kernel for review**: profile the ticks where each stage peaks, name the path and the scene its kernel needs, write that into design 13, then implement.

**Recommendation.** Option 2. No design names these kernels yet: which path a stage's worst tick spends its time on is not known, and Inputs' peak may be a few ticks of the match's start, the picks and the spawns, which a kernel of 1,000 units would not hold. The project's workflow puts a design and its review before the code.

**Blocked.** B7, B8 and B9, which stay in the plan.

