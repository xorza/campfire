# Plan — questions

## Q1. Where the binaries' end of a command line lives (C1, C2)

Each binary's `main` handles the `clap::Error` its `Args::read` gives the same way: it prints the help or the version and exits with `ExitStatus::Success`, or `Failure` when the text does not print; else it logs the usage error with `error!` and exits with `ExitStatus::Usage`. C1 writes this in the client and the server; C2 adds the verifier and the LAN check, so four copies of about fifteen lines, where U3 asks for one copy of what the binaries share. Only `common` and `log` are dependencies of all four, and neither can hold it today: `common` holds plain values only and depends on `serde` and `derive_more` alone; `log` depends on no engine crate, so it cannot name `ExitStatus`.

| Option | What changes | Cost |
| --- | --- | --- |
| **A. `log` holds it, and depends on `common`** (recommended) | `log` takes `clap` and `common`; one function, as `Logging::command_line::<Args>()`, gives the parsed command line or the exit code; design 02's dependency line changes for `log` | `log` depends on an engine crate, though one of plain values only |
| B. `log` holds it, with no exit codes | `log` takes `clap`; it gives an enum of how the command line ended, which each binary maps to `ExitStatus` | A small mapping is copied in each binary, and a second enum names the same three ends |
| C. Each binary keeps its copy | Nothing | Four copies, which U3 rules out |

Blocked: nothing. C1 keeps a copy in the client and the server, and C2 in the verifier and the LAN check, until this is decided; option A or B then replaces the four.
