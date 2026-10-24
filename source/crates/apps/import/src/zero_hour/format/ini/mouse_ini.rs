use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{BlockKinds, IniText};

/// What the import reads of Zero Hour's `Mouse.ini` files, as `Mouse::parseIni` reads them:
/// `DragTolerance`, the pixels along either axis past which a press drags, and
/// `DragToleranceMS`, the milliseconds within which a press that does not drag is a click, each 0
/// until a file sets it, as `Mouse::init` sets them.
#[derive(Debug, Default)]
pub(crate) struct MouseIni {
    pub(crate) drag_tolerance: u32,
    pub(crate) drag_tolerance_ms: u32,
}

impl MouseIni {
    /// Reads one INI file, its `Mouse` block's fields replacing those read before; its other
    /// blocks, as each cursor's, it passes over.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        for block in IniText::of(bytes).lines().blocks(BlockKinds::Any) {
            let block = block?;
            if block.head.word() != "mouse" {
                continue;
            }
            for line in &block.fields {
                let field = match line.word() {
                    "dragtolerance" => &mut self.drag_tolerance,
                    "dragtolerancems" => &mut self.drag_tolerance_ms,
                    _ => continue,
                };
                *field = u32::try_from(line.int()?)
                    .map_err(|_| IniError::Range { line: line.number })?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mouse_block_gives_the_drag_and_the_click_time_and_a_cursor_s_block_does_not() {
        let mut ini = MouseIni::default();
        assert_eq!((ini.drag_tolerance, ini.drag_tolerance_ms), (0, 0));
        ini.read(b"Mouse\n  DragTolerance = 4\n  DragToleranceMS = 167\nEnd\nMouseCursor Normal\n  DragTolerance = 7\n  DragToleranceMS = 9\nEnd\n")
            .unwrap();
        assert_eq!((ini.drag_tolerance, ini.drag_tolerance_ms), (4, 167));
        assert!(matches!(
            ini.read(b"Mouse\n  DragTolerance = -4\nEnd\n"),
            Err(IniError::Range { line: 2 })
        ));
        assert!(matches!(
            ini.read(b"Mouse\n  DragToleranceMS = -1\nEnd\n"),
            Err(IniError::Range { line: 2 })
        ));
    }
}
