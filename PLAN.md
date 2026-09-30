# Plan — next steps

Open items only, in order. Remove an item when it is done. Each step ends with the check chain passing for the crates it touches, and a stop for review. Stage context: [ROADMAP.md](ROADMAP.md); design: [design/09-determinism-core.md](design/09-determinism-core.md).

1. **Lightyear prototype**: the same schedule inside Lightyear's World, a server and one predicting client over a local connection, the log recorded on the server and replayed in the bare-`World` verifier. Exit: equal hash on every tick; measured rollback cost and Schnorr checks per packet. A failure here reopens decision 1.
