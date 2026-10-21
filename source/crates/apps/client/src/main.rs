//! Game client: joins a session over the stack's UDP, predicts the player's own avatar, and draws
//! the match; a left click or drag selects the player's own units, and a right click orders them.
//! With `--bot <orders file>`, it opens no window and renders nothing, and plays the file's
//! `OrderScript` instead. With `--key <file>`, the player's main key is the file's, made when
//! missing; without it, the player is a new key each run. With `--data <directory>`, the client
//! writes the newest receipt of its session there; without it, it writes none. With `--local`, it
//! plays the mode alone on a local server, a thread of its own process, whose data goes in
//! `server` under the data directory, `--data` then needed; each `--server-bot <slot>=<orders
//! file>` gives the server a bot there, and every other slot but the player's is open. With
//! `--spectate` too, the client holds no slot, so a bot may hold slot 0, and watches every unit as
//! the local server's one spectator; a match of bots alone starts as it joins. Space
//! pauses a local match and plays it on, minus and equals, or the keypad's minus and plus, step
//! its speed through 0.5, 1, 2 and 4 times, F5 saves it, and F9 loads its latest save.
//!
//! Logs go to standard error, filtered by `RUST_LOG` (`info`, and the renderer's warnings, by
//! default). With `CAMPFIRE_LOG` set to a path, they also go there as JSON lines, filtered by
//! `CAMPFIRE_LOG_FILTER` (Campfire's `debug` by default) with the directives of
//! `CAMPFIRE_LOG_FILTER_EXTRA` added.

use std::process::ExitCode;

fn main() -> ExitCode {
    campfire_client::run()
}
