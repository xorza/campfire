# Work record

The work of the matches at the end of each stage of the structural redesign, as design 02 asks: the instruction count of each match's test (`perf stat -e instructions:u`, the two core types of the i9-13980HX added), and each tick's wall time in an optimized test build. A stage that makes either worse by more than 10 % says why.

| After | Proving match, 600 ticks | Lane match, 900 ticks | 3v3, 2500 ticks | Proving tick: mean, p99, worst | 3v3 tick: mean, p99, worst |
| --- | --- | --- | --- | --- | --- |
| A3 | 0.84 G | 0.52 G | 8.56 G | 89 µs, 253 µs, 2.5 ms (28.6 × mean) | 111 µs, 347 µs, 26 ms (234 × mean) |

The instruction counts include each test's setup, its golden record and its checks. The worst tick's cause is not measured yet.
