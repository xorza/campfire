# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Fixes

The open issues that have no stage and that should not wait: defects that are quietly wrong, that weaken a secret or a rule of the game, or that break a rule of the code. They come before R8. Each fix that moves a golden says in its commit which golden moved and by what.

1. **F7. The verifier's errors** (verifier): the binary's `verify` returns an error enum, one variant for each failure a caller can tell apart: a package store that does not scan, a log that does not read, a log that does not decode, a replay that does not start, a snapshot that does not check. Check: a test reaches each variant and asserts it.
2. **F8. The 3v3 test's time** (runner): measure where the 8 s of `a_3v3_match_replays_to_the_same_hashes` go: the match, its replay and its copy check. Check: the runner's issue log holds a Decide item with each part's share, and the options that bring the test under 1 s, each with its cost.
3. **F14. Change trackers** (sim, runner): each world that runs the sim schedule tick after tick outside an `App`'s update, a `Runner`'s and the harness's `Arena` and `RestoreTarget`, clears Bevy's change trackers after each tick, as an `App`'s update does each frame, so the world's removed-component messages hold no removal older than the last tick, and a long replay or verification holds them in bounded memory. Check: after a match in which units die and despawn over many ticks, the removed-component messages hold only the last tick's removals; the goldens do not move.
4. **F15. The snapshot's data version** (sim): a snapshot writes the release's data version after its tag, and a restore refuses one of another version with its own `SnapshotError` case, which names both versions, as [Saves](docs/design/02-engine-core.md#saves) says; converters wait for the first release that changes the format. Check: a snapshot whose version is one past the release's is refused with that case and both numbers; the goldens do not move, as the state hash covers the bodies alone.
