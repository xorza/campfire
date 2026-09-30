# Plan — next steps

Open items only, in order. Remove an item when it is done. Each step ends with the check chain passing for the crates it touches, and a stop for review. Stage context: [ROADMAP.md](ROADMAP.md); design: [design/09-determinism-core.md](design/09-determinism-core.md).

1. **Run the Stage 2 benches** (`net` rollback, `protocol` chain-head signature) in the optimization step, and write the numbers into design 02. A rollback or check too slow for the tick budget reopens decision 1.
