# Issues

The issue log: every open problem of the project, under the step it waits for. The rules are in [AGENTS.md](../AGENTS.md#issue-log).

## Decide

## Research

## Ready

- **Net.** **Plan: N1.** `integration/replication/send_insert_0_clients` times the server app's first update: with no client, `Stepper::init` returns before any step, so the case holds the first update's setup and costs 432 µs where `send_insert_1_client` costs 96 µs (2026-10-25, local).
- **Package.** **Plan: P2.** A package on disk holds a file at every path of its index, so each duplicate is a second file: `PackageWriter::write` makes a hard link to the first file of the same SHA-256, and `OutputFile::link_new` writes a full copy where the link fails, as on FAT; a copy of the directory with `cp -r` or `robocopy` turns each link into a full copy.
