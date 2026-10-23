# The debug loop

What the work on the skirmish bots cost to debug, and how other projects find the cause of a wrong simulation, read on 23 October 2026: the engine from its code at `88983289f` and the session's command log; the others from their sources as linked.

## The session of 23 October 2026

- **Runs.** The session ran 1,002 tool calls, 932 of them shell commands. `cargo drive test` ran 63 times, 35 of them failed, 45 minutes in all, a median of 35 s; the Zero Hour check ran 34 times, 13 of them failed, 40 minutes in all, a median of 58 s; `CAMPFIRE_BLESS=1` ran 18 times.
- **Throwaway probes.** 24 of the 34 runs of the Zero Hour check came after an edit of `checks/zero-hour-check/src/match_play.rs` that printed units, which each fix then removed; 4 of those runs failed to compile: the check has no `bevy_ecs` dependency, a `ComponentInfo` gives no `name` by that call, and two closures took the wrong type. Each dump printed a `Num` as its bits, as `Num(16773965824)` for 999.8, and a component's name as `<Enable the debug feature to see the name>`.
- **A check run.** With no change, the check took 27.3 s: 15.0 s to the import's end, then 12.3 s of plays, the base play 9.8 s and the attack play 1.9 s. A change to a core crate adds a build of about 20 s.
- **The 3v3 scripted match.** It runs 12.4 to 12.9 s, the longest test, and failed 8 times, as fixes moved its golden, which names only the first tick whose hash differs, and as a pathing change moved Rime, so the showcase's Eruption was never cast. Its golden holds 15,780 ticks in 620,090 bytes.
- **Opaque pins.** The test of a bot's due orders pins each order as a list of integers, as `[[1, 5, 6, 1, 2, 2, 0, 1, 3], …]`, and failed 8 times as the session changed the bots' search.
- **Where each cause showed.** A bot's attack named a unit that no attack order takes, and every think sent it again, as the check's bot sink drops each unsent order and the orders capability ignores an order no unit takes; walkers stood inside a building's box, and at a barracks' rally three of them moved less than half a step a tick for 427 to 893 ticks; in a test, an idle friend did not step aside, as the move dropped its destination. The first two showed hundreds of ticks before the play's end, and the check reported only its end.
- **No assert runs in the check.** The check builds in `release-dev`, which inherits the release profile, so none of the 342 `debug_assert!`s of the crates runs in its plays.

## Other projects

- **Factorio's integration tests place what they test.** "Every test creates a small map" and places a few objects, runs updates and checks the result; "a crc check is made against the set of presaved crc values", which game logic changes write again ([FFF #60](https://factorio.com/blog/post/fff-60)).
- **Factorio's desync report holds both states.** A report "contains the game state from the server and the client", and the developers read their differences ([FFF #188](https://factorio.com/blog/post/fff-188)). Heavy mode saves and loads the game every tick, and on a change writes both states to a dumps folder; `level_with_tags_tick_XXXX.dat` holds the state "together with additional saving tags to aid human readability", and any text comparison tool compares them ([Factorio wiki: Desynchronization](https://wiki.factorio.com/Desynchronization)).
- **Factorio found a desync by a screenshot of its tick.** The developers narrowed a unit group's desync "deep into the movement and unit behaviour logic" from a screenshot taken "on the exact tick that the desync occurred" ([FFF #340](https://factorio.com/blog/post/fff-340)).
- **Rust prints a float by its shortest exact form.** `core::num::flt2dec`'s shortest mode gives the shortest decimal that reads back to the same float, the nearest of those, by Grisu with Dragon's fallback ([flt2dec](https://doc.rust-lang.org/src/core/num/flt2dec/mod.rs.html)).
- **Bevy's `debug` feature names types.** Without it, `bevy_utils`' `DebugName` gives `<Enable the debug feature to see the name>` (`bevy_utils-0.20.0/src/debug_info.rs`); a B0001 panic names its query by `DebugName::type_name` (`bevy_ecs-0.20.0/src/query/state.rs`). The feature adds `rand` 0.10, which the workspace's lockfile holds at 0.10.3, and lets a schedule's `shuffle_seed` shuffle the order of its unordered systems (`schedule/schedule.rs`).

## The check's build

Measured on 23 October 2026 at `88983289f`, a run of `cargo drive run zero-hour` with no change and after a change of `navigation`; the plays from the import's end to the check's.

- **`release-dev`, thin LTO, no asserts.** With no change, 27.3 s, its plays 12.3 s; a change of a core crate adds a build of about 20 s.
- **The dev profile.** With no change, 49.2 s, its plays 29.4 s, the base play 21.8 s, 2.2 times as long, and the attack play 5.1 s, 2.7 times.
- **`release-dev` with the asserts, thin LTO.** With no change, 33.0 s, its plays 17.0 s; after a change, 53.0 to 53.5 s. No assert fired.
- **`release-dev` with the asserts, no LTO.** With no change, 39.2 s, its plays 22.1 s; after a change, 41.2 to 42.1 s, so the build takes about 3 s.

## The golden's samples

Measured on 23 October 2026, the harness's golden records with a sample of their units every 300 ticks.

- **Size.** The 3v3 match's samples hold 53 samples in 4,007 lines and 213,545 bytes, beside its golden record of 620,090; the lane match's 989 bytes and the proving match's 1,530.
- **Cost.** The 3v3 match's 53 samples took 2.1 ms in all; its test ran 15.9 to 22.8 s while another process held four cores.
- **A moved line.** With one unit's life changed in the lane match's pinned samples, its test failed naming the sample at tick 0 and that line, pinned and run.

## The session again

Measured on 23 October 2026: each fix of the session taken out in turn from the tree of the debug loop, and one run of the Zero Hour check.

- **Without the move out of a static body.** The run stopped at the assert: `the walker StableId(148) stays in a static body at Vec3 { x: 1036, y: 0, z: -800 }`.
- **Without the bots' search by an attack's own test.** The base play's bot of player slot 0 gave 52 attacks that no unit took, and the attack play's 57, with 1,183 drops of no weapon; the attack play did not end in its 20 minutes.
- **Without the attack that goes on as the army falls.** The attack play ended at tick 34,740, not 17,790; its probe every 60 s showed 7 to 15 of China's units idle and up to 10 walkers stuck, but its counts do not tell units idle at home from those away, so the cause needed its rows' places.
- **The showcase from its setup.** The 3v3's shop and showcase, from heroes given level 6 and gold by the mode's script, played 1,806 ticks in 0.25 s; Snow Owl, aimed at (0, 15) as in the long match, started late, as its hero stood 35 m away, past its 25 m, and now aims at (0, 0).

## The harness and the check

Read on 23 October 2026, the code at `3d40814ba` and the working tree of the debug loop.

- **Two bot drivers.** The server's `BotDriver` (`host/src/server/bot_driver.rs`) plays `TickBots` into its session through a sink that logs each unsent order, failed think and dropped payload as an event, and drops the think's operation count; the Zero Hour check's `MatchPlay` plays them through a sink of its own, which collects the payloads, serves them after the play, counts the unsent orders and keeps the most operations.
- **Failed calls already fail a match.** `SessionWorld::run_tick_alone` logs each failed script call of the tick as `ScriptCallFailed`, a warning, and `LogCheck` fails a test on each warning no test takes; five test files assert besides, after each tick, that `ScriptFailures` is empty.
- **Ticks around `FixedMatch`.** 33 calls in the harness's tests and the check run a tick by `game_mut().run_tick_alone()`, not by `FixedMatch::step`.
- **Repeated queries.** The units of a type a player slot owns: in the check's `MatchPlay::owned`, `skirmish.rs`'s `owned` and `one`, and the lane test's `unowned`; a player's amount of a resource: `moba.rs`'s `gold`, `skirmish.rs`'s `money` and the check's `MatchPlay::money`; picks of a faction for each slot from text: the check's `america` and `skirmish.rs`'s `bases`.
- **The Zero Hour plays record nothing.** Unlike the harness's scripted matches, the check's plays keep no `MatchRecord`: no copy check of each tick, no golden and no replay of their log.

## The bot plays' record

Measured on 23 October 2026, the Zero Hour check's base play and attack play, each run twice, with a `MatchRecord` of each tick and the replay of its log, and without.

- **Without a record.** The base play took 17.9 and 18.0 s, the attack play 4.1 and 4.2 s.
- **With a record.** The base play took 70.2 and 71.4 s, the attack play 18.7 and 18.8 s; every tick's copy checked and each log replayed to the same hashes.
- **With a record of no golden.** With a `ReplayRecord`, the copy check and the replay alone, the base play took 71.0 s and the attack play 18.6 s: the copy check is the cost.
