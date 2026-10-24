# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

- **Mode.** The refusals of a rules script's call of `win`, `draw` or `choose` and of its read of the mode's state have no test: the load refuses every script that names one, and through `ctx` under another name too, so no loaded package reaches them.
- **Stats.** A perf profile of `server_tick/mean_3v3` (2 kHz, frame pointers, one run of 8 s) put 24.8 % of its samples in `Refresh::run`'s collection of the units its change filter selects, where `.notes/PERF.md` measured the whole refresh at 3.4 % of the 3v3 tick; the same profile put 13.5 % in the kernel's file lookups, which no tick makes.
- **Navigation.** In the Zero Hour check's base play, GLA's tunnel defender #230 walks from (1194.6, −2175.1) toward (1076.1, −1768.8) stuck for 17,839 ticks, about 10 minutes, at 1,140 s.

## Ready

- **Projectiles.** A tick in which no projectile flies changes `projectiles.struck_units`, so the state copy of a quiet tick of a mode with a projectile type carries it.
