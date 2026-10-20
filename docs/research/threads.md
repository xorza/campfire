# Threads — research

Which threads the workspace starts, how Bevy sizes its task pools, and what each app does with them, for the threads of [Storage](../design/02-engine-core.md#storage) and [Bevy](../design/02-engine-core.md#bevy). Facts only. Read on 20 October 2026 from the workspace at `0f2cd7a13` and the crates in its `Cargo.lock`.

## The workers

Every OS thread of Campfire's own starts through `campfire_store::Worker`; `clippy.toml` bans `std::thread::spawn`, `std::thread::scope` and each `std::thread::Builder::spawn*`.

| Name | Started by | Crate | Job |
| --- | --- | --- | --- |
| `log` | `StreamWriter::create` | `log` | Writes the process's log file |
| `journal` | `AppendWriter::create`, `reopen` | `net` | Appends and syncs the session's journal |
| `checkpoints` | `Exchange::start` | `net` | Applies, encodes, hashes and writes a checkpoint |
| `receipts` | `LatestWriter::start` | `net` | Writes the client's latest receipt |
| `local server` | `Worker::start` | `net` | Runs the singleplayer server's app |
| `campfire-endpoint` | `Worker::start` | `net-endpoint` | Waits on the UDP socket and runs the packet sessions' timers |
| `campfire-port-map` | `Worker::start` | `net-port-map` | Maps the server's port at the router |

- **Names past the OS's bound.** Linux keeps 15 bytes of a thread's name (`TASK_COMM_LEN`, 16 with its NUL): `campfire-endpoint` and `campfire-port-map` are 17 bytes, and tools as `top`, `perf` and Nsight Systems show them cut.
- **Why `store` for `net-packets` and `net-endpoint`.** Of `store`, `net-packets`, a problem crate, and `net-endpoint`, a glue crate, name `Worker` alone; `net-port-map` names `Worker` and `SharedPort`.
- **Signals start no thread.** `store`'s stop signals register through `signal_hook::flag`, which sets an atomic flag in the handler.

## Bevy's task pools

Read from `bevy_app` 0.20.0 and `bevy_tasks` 0.20.0.

- **One set for each process.** `ComputeTaskPool`, `AsyncComputeTaskPool` and `IoTaskPool` are statics that `TaskPoolPlugin` fills with `get_or_init`: the first app a process builds sizes them, and a later app, as the singleplayer client's local server, takes them as they are (`bevy_tasks/src/usages.rs`).
- **Default sizes.** `TaskPoolOptions::default` gives the IO pool 25 % of `available_parallelism`, from 1 to 4 threads, the async compute pool the same, and the compute pool what is left, at least 1; on a machine of 32 logical cores that is 4 + 4 + 24 = 32 threads, each started as the pools are made (`bevy_app/src/task_pool_plugin.rs`). `TaskPoolOptions::with_num_threads(1)` gives each pool 1.
- **Who sets the sizes.** The client's window end, through `DefaultPlugins`, its bot end, `SimServerPlugins` and the net harness's `InProcessMatch` add `TaskPoolPlugin::default()`; the stack's stepper adds `with_num_threads(1)`; nine test apps add `TaskPoolPlugin::default()`.
- **Who sets the executor.** Four places set `SingleThreadedExecutor` on schedules, each with its own loop: the server's `OneThreadPlugin`, the sim's tick (`SimUpdate`), the stepper's `run_strictly` and the net harness's `InProcessMatch`.
- **Who runs in parallel.** Four systems of Campfire's code run a query on the compute pool: through `net-transport`'s `AdaptiveQueryIterMut`, which calls `par_iter_mut` when a query holds 2 links or more, the transport's send and receive systems and `net-messages`' send and receive systems, on the server too, whose executor is single-threaded.

## Outside crates

- **`tokio`.** In `Cargo.lock` through `wasm-bindgen-futures`, which `bevy_platform` takes only for WebAssembly; `cargo tree -e all --target all -i tokio` reaches it from no native build.
