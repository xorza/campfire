use campfire_common::Tick;
use campfire_log::LogEvent;
use campfire_script::{PrintKind, ScriptPrint};
use serde::Deserialize;
use tracing::debug;

/// A script of the match printed `text` in `tick`: with `print`, or with `debug`, at `line` and
/// `column` when Rhai knows them. The server logs it at `debug` after that tick; only a debug
/// build records a print.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub(crate) struct ScriptPrinted {
    pub tick: Tick,
    pub text: String,
    #[serde(default)]
    pub debug: bool,
    #[serde(default)]
    pub line: Option<u32>,
    #[serde(default)]
    pub column: Option<u32>,
}

impl ScriptPrinted {
    /// The print `print` of `tick`, as the server logs it.
    pub(crate) fn of(tick: Tick, print: ScriptPrint<'_>) -> Self {
        let (debug, line, column) = match print.kind {
            PrintKind::Print => (false, None, None),
            PrintKind::Debug { line, column } => (true, line, column),
        };
        Self {
            tick,
            text: print.text.to_owned(),
            debug,
            line,
            column,
        }
    }
}

impl LogEvent for ScriptPrinted {
    const MESSAGE: &'static str = "a script printed";

    fn log(&self) {
        let (tick, text) = (self.tick.get(), self.text.as_str());
        if self.debug {
            let (line, column) = (self.line, self.column);
            debug!(tick, text, debug = true, line, column, "{}", Self::MESSAGE);
        } else {
            debug!(tick, text, "{}", Self::MESSAGE);
        }
    }
}

#[cfg(test)]
mod tests {
    use campfire_log::internals::round_trip;

    use super::*;

    #[test]
    fn the_event_reads_back_what_it_logs() {
        let tick = Tick::new(30);
        let print = ScriptPrinted::of(
            tick,
            ScriptPrint {
                kind: PrintKind::Print,
                text: "wave 2",
            },
        );
        let debug = ScriptPrinted::of(
            tick,
            ScriptPrint {
                kind: PrintKind::Debug {
                    line: Some(4),
                    column: None,
                },
                text: "#{a: 1}",
            },
        );
        assert_eq!((print.debug, print.line, print.column), (false, None, None));
        assert_eq!(
            (debug.debug, debug.line, debug.column),
            (true, Some(4), None)
        );
        round_trip(&print);
        round_trip(&debug);
    }
}
