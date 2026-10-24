# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide


## Research

- **Navigation.** In the bench scene `integration/navigation/*_crowd` (`capabilities/src/bench/navigation.rs`, `Scene::Crowd`), unit #366, an idle friend that stepped aside 1 deep, stands from about tick 1,280 with a destination 0.55 m off, no waypoint ahead and a fresh `Progress`: it never moves, never counts as stuck, and keeps its destination for good.
- **Net.** On CI's `windows-2025`, each of the 10 tests of `campfire-net-stack`'s `alloc` binary (`fixtures::idle`, `fixtures::messages`, `fixtures::prediction`, `fixtures::replication`) times out at 30 s, so the gate fails there; they pass on `ubuntu-26.04` and `macos-26`. Seen in runs 40120124881 (`68e772d55`) and 40128753732.

## Ready

