use campfire_client_files::{KeysFile, Press};

use crate::zero_hour::format::ini::mouse_ini::MouseIni;

/// The milliseconds of a second.
const MILLISECONDS: f32 = 1000.0;

/// The imported package's keys: the engine's own, but its press, `mouse`'s, which drags past its
/// `DragTolerance` pixels and clicks within its `DragToleranceMS` (02-engine-core.md, Keys are the
/// mode's data). The game's pixels are taken as logical pixels.
#[expect(
    clippy::cast_precision_loss,
    reason = "the pixels and the milliseconds `Mouse.ini` sets are far below 2²⁴, exact in f32"
)]
pub(crate) fn keys_file(mouse: &MouseIni) -> KeysFile {
    KeysFile {
        press: Press {
            drag: mouse.drag_tolerance as f32,
            click: mouse.drag_tolerance_ms as f32 / MILLISECONDS,
        },
        ..KeysFile::default()
    }
}

#[cfg(test)]
mod tests;
