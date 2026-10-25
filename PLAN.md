# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Each SHA-256 once

A package holds one file of each SHA-256 its index lists, at the first path of it that it holds, and no link or copy at the others; the index is the one record of the others, as two rows of one SHA-256 name the same bytes. A batch in `package-format`, `package-disk` and `store`, taken from the first batch of [the package archive's proposal](.notes/package-archive.md#batch-1-each-sha-256-once), which the archive builds on. The index's format and every fingerprint stay as they are.

### P2. The writer writes each SHA-256 once (package-disk's writer, store)

- [ ] `PackageWriter::write` makes a hard link to the first file of the same SHA-256, or a full copy where the link fails, as on FAT (`package_writer/mod.rs:56`) → it writes the first path of each SHA-256 alone and keeps only the row of each later path. `OutputFile::link_new` (`store/src/output_file/mod.rs:45`) loses its last caller and goes, with its test. Builds: what can be forgotten is a type, no copy for `cp -r` or `robocopy` to multiply. Takes the **Package** issue.
- [ ] `05-protocol-spec.md`'s "A package holds each SHA-256 once" drops its mark of the writer's part as planned. The proposal's decision "Each SHA-256 once, in every form" and its first batch leave it, pointing to the protocol.
- Checks: a package the writer makes of two paths of the same bytes holds one file, and loads with both rows; the Zero Hour import's packages hold one file of each SHA-256, and `cargo drive run zero-hour` passes with no golden moved.

## A ready stepper has stepped

`net-stepper` alone, a batch.

### N1. `Stepper::init` steps the server once at least (net-stepper's stepper)

- [ ] With no client, `Stepper::init` returns before any step (`stepper/mod.rs:349`), so a stepper made `Init::Ready` holds a server that has never updated, and `integration/replication/send_insert_0_clients` times that first update with its setup, 432 µs where `send_insert_1_client` costs 96 µs → `init` steps until the server has run an update and each client is ready, so `Init::Ready` means the same with no client as with any. Builds: what can be forgotten is a type, `Init::Ready`'s promise kept by `init`, not by each caller. Takes the **Net** issue of the bench.
- Checks: a stepper of no client made ready has run its server's first update, as one of one client has; `send_insert_0_clients` costs no more than `send_insert_1_client`.

## Foundations
