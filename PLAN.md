# Plan — next steps

Open items only, in order. Remove an item when it is done. Each step ends with the check chain passing for the crates it touches, and a stop for review. Stage context: [ROADMAP.md](ROADMAP.md); design: [design/](design/).

1. **CI on three platforms**: a GitHub Actions workflow runs the check chain (`fmt`, `clippy`, `test`) on Linux x86_64, Windows x86_64 and macOS aarch64, with the Linux packages Bevy's window backends need. The Linux job also runs `campfire-lan-check` and keeps the session log and its hash as an artifact; the Windows and macOS jobs verify that log with their own `campfire-verifier` and compare the hash. This closes stage 0, and the golden and prototype tests then show equal hashes on every platform, as stage 2 asks.
2. **Vertical slice playtest**: a 1v1 on LAN between two machines, with the test heroes' abilities, the players' attack and cast orders and the readable match, and the log verified on a third OS. Write down what makes the match fun or not, and which items stage 3 still needs. This closes stage 3, or names what is missing.
