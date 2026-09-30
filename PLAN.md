# Plan — next steps

Open items only, in order. Remove an item when it is done. Each step ends with the check chain passing for the crates it touches, and a stop for review. Stage context: [ROADMAP.md](ROADMAP.md); design: [design/](design/).

1. **CI on three platforms**: a GitHub Actions workflow runs the check chain (`fmt`, `clippy`, `test`) on Linux x86_64, Windows x86_64 and macOS aarch64, with the Linux packages Bevy's window backends need. The Linux job also runs `campfire-lan-check` and keeps the session log and its hash as an artifact; the Windows and macOS jobs verify that log with their own `campfire-verifier` and compare the hash. This closes stage 0, and the golden and prototype tests then show equal hashes on every platform, as stage 2 asks.
2. **Vertical slice playtest**: a 1v1 on LAN between two machines, with the test heroes' abilities, the players' attack and cast orders and the readable match, and the log verified on a third OS. Write down what makes the match fun or not, and which items stage 3 still needs. This closes stage 3, or names what is missing.
3. **LAN check failures that name their cause**: in one of six runs, right after a large rebuild, the check failed, and the next run deleted its logs. Only the last failure line was read, "the server did not write the session log", which follows from any earlier failure. A likely cause is a connection that failed under load: a client reacts to `Disconnected` only after it asked to leave, so a failed connection waits for the 30 s deadline and logs nothing. Three changes:
   - A client that gets `Disconnected` before it asked to leave logs an error with lightyear's reason and exits with failure.
   - Each run writes into a new directory, named for its start time, below the run root that the command line names; no run deletes the logs of another run. `verify` still takes the directory of one run.
   - The verdict does not report a failure that follows from another one: no "no session log" and no "no final hash" when the server did not succeed. The last line gives the number of failures and the run's directory.

   Test: a bot with a wrong certificate exits with failure in a few seconds, and its log holds the reason; the verdict of a server that overran holds no session log failure; two runs keep two directories.
