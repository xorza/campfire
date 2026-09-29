# Plan — next steps

Open items only, in order. Remove an item when it is done. Each step ends with the check chain passing for the crates it touches, and a stop for review. Stage context: [ROADMAP.md](ROADMAP.md); design: [design/09-determinism-core.md](design/09-determinism-core.md).

1. **Inputs and the session log** in `protocol`, in memory only: a per-tick input list with the hash chain per player and the applied-tick rule; signatures come later. Test: a dropped input breaks the chain, and a replay reads the inputs back in order.
2. **Headless prototype**: one unit that moves on move orders, run by a minimal `runner` from a session log, and replayed by `verifier` in a bare `World`. Test: equal state hash on every tick between the run and the replay. This is the first half of the Stage 2 gate, without the network.
3. **Lightyear prototype**: the same schedule inside Lightyear's World, a server and one predicting client over a local connection, the log recorded on the server and replayed in the bare-`World` verifier. Exit: equal hash on every tick; measured rollback cost and Schnorr checks per packet. A failure here reopens decision 1.
