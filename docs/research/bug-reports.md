# Bug reports

What a report of a bug the player saw needs, and how other projects take one, read on 25 October 2026: the engine from its code as the working tree held it; the others as cited.

## The engine

- **The log is published at the session's end.** A server appends each record to its journal; `SessionDir::publish` streams the journal twice into the log file, and needs the server seed of the last segment, which the session reveals only as it ends (`session/host/src/server/session_dir/mod.rs`, `session/protocol/src/session_log/publish.rs`).
- **A save is a local server's alone.** `Checkpoints::take` refuses a save on any server but a local one, and a save is a checkpoint in the journal, its snapshot and the log before it, not a file of its own (`session/host/src/server/checkpoints/mod.rs`).
- **A local match runs its server in the client's process.** `--local --data <dir>` keeps the server's journal, snapshots and published logs under `<dir>/server`; the local keys reach it through `PendingSaves`, as F5 saves and F9 loads (`apps/client/src/local_keys.rs`).
- **The camera is a rig.** `CameraRig` holds a pivot on the ground plane, a yaw, a pitch and a height, which place the camera, and the camera file's limits (`apps/client/src/view/camera_rig/mod.rs`).
- **Bevy captures a window's frame.** An entity of `Screenshot::primary_window()` triggers `ScreenshotCaptured` with the frame as an `Image`; `save_to_disk` encodes it with the `image` crate and writes it with `std::fs` (`bevy_render-0.20.0/src/view/window/screenshot.rs`).
- **No PNG encoder is built.** The lockfile holds `image` 0.25.10 through `bevy_image`, of no `png` feature, and no `png` crate. A frame of the 7680 × 2160 monitor is 66 MB of RGBA uncompressed.
- **No tool replays a published log to a tick.** The verifier replays a log whole to check its hashes; the Zero Hour check's probes table a play's units, not a log's.

## Other projects

- **Unreal's `BugIt` saves the view and gives it back.** `UCheatManager::BugIt` takes a screenshot and logs the camera's location and rotation as a `BugItGo X Y Z Pitch Yaw Roll` line, which puts the camera back there.
- **Factorio's desync report holds both states,** the server's and the client's, which the developers compare ([FFF #188](https://factorio.com/blog/post/fff-188)), and it found a desync from a screenshot of its exact tick ([FFF #340](https://factorio.com/blog/post/fff-340)).
