# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Command lines

The design is [Structure and names](docs/design/12-structure.md), U12: every binary reads its command line with `clap`. No command line changes; each binary gains `--help` and `--version`, and its usage errors take clap's text.

1. **C1. The client and the server** (workspace, client, server, log; [U12](docs/design/12-structure.md#decisions)): `clap` `=4.6.7` as a workspace dependency, its default features off, with `std`, `derive`, `help`, `usage`, `error-context` and `suggestions`; each `Args` derives `Parser`, and the flag loops, `Flag` and the cases clap states go, the client's `ArgsError` keeping `BotInClientSlot`; `main` prints `--help` and `--version`, and logs a usage error; `log`'s test lists `clap::Error` as its second exception. Tests: the tables of each binary's arguments, each refusal by its `ErrorKind` or its parser's error; `--help` exits with 0.
2. **C2. The verifier and the LAN check** (verifier, lan-check; [U12](docs/design/12-structure.md#decisions)): the verifier's arguments, and the LAN check's run root and `verify <run directory>`, through `clap`. Tests: the LAN check's mode test, each refusal by its `ErrorKind`.
