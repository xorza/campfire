# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Command lines

The design is [Structure and names](docs/design/12-structure.md), U12: every binary reads its command line with `clap`. No command line changes; each binary gains `--help` and `--version`, and its usage errors take clap's text.

1. **C2. The verifier and the LAN check** (verifier, lan-check; [U12](docs/design/12-structure.md#decisions)): the verifier's arguments, and the LAN check's run root and `verify <run directory>`, through `clap`. Tests: the LAN check's mode test, each refusal by its `ErrorKind`.
