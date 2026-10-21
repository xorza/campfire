# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research
- **Net messages.** The stack's tests send messages of the test protocol that no system of the receiving app reads, so each link logs `Unhandled messages` at Warn in every frame: 1,009 times in one run of the tests.
- **Net UDP.** A client's first sealed packet that a link's jitter brings to the server before the last packet of its handshake meets no route, so the server answers it with a stateless reset of the connection ID it gave the client, and the client's live connection ends: the network harness's 1v1 through its delayed links shows it with the seeds 5 and 9.

## Ready
- **Net prediction.** The correction systems join a set and also its parent set, so each app that adds them logs at Warn, as its `RunFixedMainLoop`, `PostUpdate` and `FixedPostUpdate` build, that their hierarchy holds redundant edges.
- **Net stepper.** The stepper holds no log check, so a stack test passes with the events at Warn and Error its apps log, those of the two items above among them, and `a_client_in_both_targets_is_predicted_alone` and `a_duplicate_pre_spawn_hash_keeps_the_first_candidate` take none of the ones they cause.
