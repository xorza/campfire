# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

## Ready

- **Net.** `integration/replication/send_insert_0_clients` times the server app's first update: with no client, `Stepper::init` returns before any step, so the case holds the first update's setup and costs 432 µs where `send_insert_1_client` costs 96 µs (2026-10-25, local).
- **Net.** **Plan: A1.** On CI's `windows-2025`, each of the 10 tests of `campfire-net-stack`'s `alloc` binary that traces allocations times out at 30 s, so the gate fails there; they pass on `ubuntu-26.04` and `macos-26`. Seen in runs 40120124881 (`68e772d55`) and 40128753732.
- **Package.** A package on disk holds a file at every path of its index, so each duplicate is a second file: `PackageWriter::write` makes a hard link to the first file of the same SHA-256, and `OutputFile::link_new` writes a full copy where the link fails, as on FAT; a copy of the directory with `cp -r` or `robocopy` turns each link into a full copy. The load asks the source for every path of the index (`PackageFiles::read_indexed`, `check_names`), and `PackageDir::read` opens each path's own file, so a directory that holds one file of each SHA-256 fails as `Missing`, though its index names the bytes of every path.
