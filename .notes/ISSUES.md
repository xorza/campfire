# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

## Ready
- **Net interpolation.** **Plan: Net 1.** Frame interpolation's typed systems join their phase set and also the stage's set that holds it, so each app with frame interpolation logs at Warn, as its `RunFixedMainLoop`, `PostUpdate` and `FixedPostUpdate` build, that their hierarchy holds redundant edges; the strict check covers only `Update` and `FixedUpdate`.
- **Net messages.** **Plan: Net 1.** Four tests leave messages that no system of the receiving app reads, so each frame logs `Unhandled messages` at Warn: the wire-cost scenario's, the replicon receiver's removal test's, the server's stop test's `SessionOffer`, which `ProtocolClient` never reads, and the test of an unread message's drop, which no typed event lets take its warning; 1,009 times in one run of the tests.
- **Net stepper.** **Plan: Net 2.** The stepper holds no log check, so a stack test passes with the events at Warn and Error its apps log, those of the two items above among them, and `a_client_in_both_targets_is_predicted_alone` and `a_duplicate_pre_spawn_hash_keeps_the_first_candidate` take none of the ones they cause.
- **Net UDP.** **Plan: Net 3.** A client's first sealed datagram that a link's jitter brings to the server before the last packet of its handshake finds no packet session's route, and the server answers with a stateless reset of the connection ID its pending handshake gave the client, so the client's connection ends: the network harness's 1v1 through its delayed links shows it with the seeds 5 and 9.
