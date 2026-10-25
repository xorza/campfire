# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Each SHA-256 once

A package holds one file of each SHA-256 its index lists, at the first path of it that it holds, and no link or copy at the others; the index is the one record of the others, as two rows of one SHA-256 name the same bytes. A batch in `package-format`, `package-disk` and `store`, taken from the first batch of [the package archive's proposal](.notes/package-archive.md#batch-1-each-sha-256-once), which the archive builds on. The index's format and every fingerprint stay as they are.

### P1. A row reads from the first path of its SHA-256 that the package holds (package-format's reader, package-disk's sources)

First, as a package the writer makes after P2 loads only by it.

- [ ] The open takes a row only by its own path: `PackageFiles::read_indexed` gives `check_names` every path of the index and fails on the first the source lacks (`package_files.rs:49`), and `PackageReader::read_row` reads each row at its own path (`package_reader.rs:85`), so a directory that holds one file of each SHA-256 fails as `Missing` → the source answers which of the index's paths it holds by exact name, as `Listings::hold` answers for a directory (`package_dir/listings.rs:20`) and `TreeDir` from its map, and the open maps each row to the first held path of its SHA-256 in the index's order, kept beside the index, which every read takes; a row is `Missing` only when no path of its SHA-256 is held (package-archive.md, Each SHA-256 once, in every form). Builds: a name is resolved once, the map made at the open, and no read searches. Takes the **Package** issue.
- Checks: a directory and a `TreeDir` that hold one file of each SHA-256 load, and each row of a duplicate reads its bytes; one whose first path of a SHA-256 is missing and second is held loads from the second; a row whose SHA-256 no held path has is `Missing` by its own path; a path held only in another case is not held, so a package that loads on Linux loads on macOS and Windows; an authored tree with a copy at every path still loads.

### P2. The writer writes each SHA-256 once (package-disk's writer, store)

- [ ] `PackageWriter::write` makes a hard link to the first file of the same SHA-256, or a full copy where the link fails, as on FAT (`package_writer/mod.rs:56`) → it writes the first path of each SHA-256 alone and keeps only the row of each later path. `OutputFile::link_new` (`store/src/output_file/mod.rs:45`) loses its last caller and goes, with its test. Builds: what can be forgotten is a type, no copy for `cp -r` or `robocopy` to multiply.
- [ ] `05-protocol-spec.md`'s Packages gains "A package holds each SHA-256 once": the rule of P1 and P2, as a git tree names one blob for every path of the same bytes; refused: a hard link or a copy at each other path, which `cp -r`, `robocopy` and an archiver turn into full copies and FAT into a copy at the write; a table of aliases beside the index, which holds what two rows of one SHA-256 already say. The proposal's decision "Each SHA-256 once, in every form" and its first batch leave it, pointing to the protocol.
- Checks: a package the writer makes of two paths of the same bytes holds one file, and loads with both rows; the Zero Hour import's packages hold one file of each SHA-256, and `cargo drive run zero-hour` passes with no golden moved.

## A ready stepper has stepped

`net-stepper` alone, a batch.

### N1. `Stepper::init` steps the server once at least (net-stepper's stepper)

- [ ] With no client, `Stepper::init` returns before any step (`stepper/mod.rs:349`), so a stepper made `Init::Ready` holds a server that has never updated, and `integration/replication/send_insert_0_clients` times that first update with its setup, 432 µs where `send_insert_1_client` costs 96 µs → `init` steps until the server has run an update and each client is ready, so `Init::Ready` means the same with no client as with any. Builds: what can be forgotten is a type, `Init::Ready`'s promise kept by `init`, not by each caller. Takes the **Net** issue of the bench.
- Checks: a stepper of no client made ready has run its server's first update, as one of one client has; `send_insert_0_clients` costs no more than `send_insert_1_client`.

## Foundations
