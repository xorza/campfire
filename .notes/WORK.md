# Work record

The work of the matches at the end of each stage of the structural redesign, as design 02 asks: the instruction count of each match's test (`perf stat -e instructions:u`), and each tick's wall time in an optimized test build. A stage that makes either worse by more than 10 % says why.

From B4 on, the built test binary runs pinned to one performance core of the i9-13980HX (`taskset -c 2`, never on `cargo`), with one test thread: its instruction count is then the same in every run, where the A3 counts, from both core types, moved by up to 50 % between runs. The tick times are the third of three runs in one process, once the clock settled: the first run's times are up to twice as long.

| After | Proving match, 600 ticks | Lane match, 900 ticks | 3v3, 2500 ticks | Proving tick: mean, p99, worst | 3v3 tick: mean, p99, worst |
| --- | --- | --- | --- | --- | --- |
| A3 | 0.84 G | 0.52 G | 8.56 G | 89 µs, 253 µs, 2.5 ms (28.6 × mean) | 111 µs, 347 µs, 26 ms (234 × mean) |
| B4 | 0.812 G | 0.512 G | 7.169 G | 52 µs, 154 µs, 1.3 ms (25.1 × mean) | 42 µs, 115 µs, 2.8 ms (67.2 × mean) |

The instruction counts include each test's setup, its golden record and its checks.

At B4 the worst tick of both matches is tick 0, the schedule's first run, which builds each system's state: 1.3 ms in the proving match, 2.8 ms in the 3v3. The 3v3's next worst is tick 2400, after the first wave spawns, at 0.98 ms, 23 × the mean, then ticks 2399, 1199 and 1200, near 0.25 ms. The 3v3's count fell by 16 % in stage B, as `resolve_casts`, `think` and `finish_trains` no longer walk every entity.
