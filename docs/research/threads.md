# Threads — research

How Bevy sizes its task pools, for the threads of [Storage](../design/02-engine-core.md#storage) and [Bevy](../design/02-engine-core.md#bevy). Facts only. Read on 20 October 2026 from the crates in the workspace's `Cargo.lock` at `0f2cd7a13`.

## Bevy's task pools

Read from `bevy_app` 0.20.0 and `bevy_tasks` 0.20.0.

- **One set for each process.** `ComputeTaskPool`, `AsyncComputeTaskPool` and `IoTaskPool` are statics that `TaskPoolPlugin` fills with `get_or_init`: the first app a process builds sizes them, and a later app, as the singleplayer client's local server, takes them as they are (`bevy_tasks/src/usages.rs`).
- **Default sizes.** `TaskPoolOptions::default` gives the IO pool 25 % of `available_parallelism`, from 1 to 4 threads, the async compute pool the same, and the compute pool what is left, at least 1; on a machine of 32 logical cores that is 4 + 4 + 24 = 32 threads, each started as the pools are made (`bevy_app/src/task_pool_plugin.rs`). `TaskPoolOptions::with_num_threads(1)` gives each pool 1.
