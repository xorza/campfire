use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::platform::os::Os;

/// The signals that stop a process by its exit, caught for the rest of the process. On Unix,
/// `SIGINT`, `SIGTERM` and `SIGHUP`, each only when the process did not start with it ignored, as
/// `nohup` starts one with `SIGHUP` and a shell's `&` with `SIGINT`; on Windows, the console's
/// Ctrl-C, Ctrl-Break, close, logoff and shutdown, where a process started in a group of its own
/// takes no Ctrl-C. The first raises the flag `received` reads; a second ends the process at once
/// with `EXIT_CODE`. Windows ends a process once the handler of its console's close returns, so
/// that handler waits for the process's exit, within the 5 s Windows gives.
#[derive(Debug)]
pub struct StopSignals {
    received: Arc<AtomicBool>,
}

impl StopSignals {
    /// The code a stopped process exits with: 130, `128 + SIGINT`, as a shell gives an
    /// interrupted job.
    pub const EXIT_CODE: u8 = 130;

    /// Catches the stop's signals, once in a process; an error when the OS refuses a handler.
    pub fn catch() -> io::Result<StopSignals> {
        let received = Arc::new(AtomicBool::new(false));
        Os::catch_stop(&received)?;
        Ok(StopSignals { received })
    }

    /// Whether a stop's signal arrived.
    pub fn received(&self) -> bool {
        self.received.load(Ordering::SeqCst)
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use std::process::{Child, Command};

    use crate::platform::os::Os;
    use crate::platform::stop_signals::StopSignals;

    impl StopSignals {
        /// Makes `command` start a child that `stop` stops, with the signals of a terminal
        /// ignored: on Unix, `SIGINT` and `SIGHUP`, as `nohup` and a shell's `&` leave them; on
        /// Windows, Ctrl-C, in a process group of its own, where Ctrl-Break reaches it alone.
        pub fn ignoring_terminal(command: &mut Command) -> &mut Command {
            Os::ignore_terminal(command);
            command
        }

        /// Sends `child`, which `ignoring_terminal` started, the stop's signal no terminal sends:
        /// `SIGTERM` on Unix, Ctrl-Break to its group on Windows.
        pub fn stop(child: &Child) {
            Os::stop(child.id()).unwrap();
        }
    }
}
