# Plan questions

## A client's re-simulated 3v3 tick (B12)

**Item.** B12 asks for a case that times a client's re-simulated 3v3 tick, so design 02's bound of an 8-tick rollback, 10 ms from eight of the server's worst ticks, comes from a measurement. The proposal is in design 13, [A client's re-simulated 3v3 tick](docs/design/13-benches.md#a-clients-re-simulated-3v3-tick), for review before the case is built.

**Options.**

1. **An end case**: `InProcessMatch` plays any mode's packages, with one predicting client and the server's bots in the other slots; `client_frame/walk_3v3` and `walk_rollback_3v3`, whose difference over a rollback's depth is a re-simulated tick. Faithful to what a client pays, Lightyear's restore included; it changes `net`'s harness, and its fixture plays 2,400 networked ticks in CI's dev profile.
2. **A kernel**: a client's world built as `SimClient` builds it, filled with the server's state at a wave's tick, every unit unpredicted but one hero; `client_resim/3v3` times its `SimUpdate`. No network and cheap, but the full state overstates what replication sends, and Lightyear's restore is left out.

**Recommendation.** Option 1, with option 2 as the fallback if CI's time cannot carry the fixture.

**Blocked.** B12's case, which stays in the plan.
