# Server tick profile

Where the MOBA 3v3's mean server tick spends its time, sampled 2026-10-18 at `80dc76d07`.

## Machine and method

- **Machine:** Intel Core i9-13980HX, Linux, x86-64-v3, a laptop on mains power, with other work running.
- **Build:** the bench profile with frame pointers in its own target directory (02-engine-core.md, Frame pointers for profiling).
- **A run:** `perf record -F 2000 -g --call-graph fp` of `integration/server_tick/mean_3v3` with `--profile-time 20`; 40,310 samples. The shares below are of every sample, the bench's setup of each match among them; the timed ticks hold 68 % of them.

## Results

| What | Share of the samples, with its callees |
| --- | ---: |
| The ticks the bench times | 68.4 % |
| The AI's think batch | 26.9 % |
| Rhai's calls of script functions | 25.4 % |
| Script view reads | 11.6 % |
| Vision's system that reads the relations through `OnMatch` | 6.6 % |
| The damage pass, its script batch among it | 4.5 % |
| `ScriptCall::begin`, the check of the tick's draws at each call | 1.9 % |
| `Filter::take_text`, a filter's text parsed at each call | 1.5 % to 2.5 % |
| `PlayersCall::begin`, the players' resources copied at each call | 0.35 % |

- No lookup of the entity index's B-tree for the match's or a player's entity shows on its own: each is under the 0.5 % of a line the report shows, so the index's lookups cost little next to the work around them.
- The damage pass's own reads of the units are 0.13 % of the samples; the rest of its 4.5 % is its script batch.
