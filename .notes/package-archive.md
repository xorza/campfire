# Package archives

A slice's proposal: a package as one zip archive that the engine reads in place. At its end, an import of Zero Hour writes `game.zip` and `zero-hour.zip`, and the client plays a skirmish from them with no unpacked files.

It lives in `.notes/` until its review; when the slice ends, each lasting decision moves into the document of the system it governs ([the last batch](#batch-4-the-import-and-the-programs)), and this file is deleted. No code cites it before then.

## Decisions

- **A package may be one zip archive.** A package is a directory, or a file `<name>.zip` whose root holds the same tree, `package.index` among it, as Android's APK, Java's JAR, Quake 3's `.pk3` and Factorio's mods are zip archives their programs read in place. Refused: a container of Campfire's own, which no archiver opens; tar, which has no table of its entries, three header formats, and no reader in Windows' Explorer; SQLite's `sqlar`, which needs SQLite, a C library ([Assets are untrusted](../docs/design/03-game-scripting.md#game-package)); IPFS's CAR, whose block hashes are not the index's file hashes.
- **The index stays the identity.** `package.index` is the same file in a directory, in an archive and as a blob, so a package has one fingerprint in every form ([A package is its index](../docs/design/05-protocol-spec.md#packages)). Refused: zip's CRC-32 as the check of a file, which four appended bytes set to any value, as CRC is linear, and which two files share at a chance of 2⁻³²; a SHA-256 in each entry's extra field, whose fingerprint would need a canonical list of them, which is the index again, and which the directory and the blobs do not have.
- **The archive's bytes are not identity.** The writer fixes each timestamp at 1980-01-01, the DOS epoch, and each entry's mode at a regular file's `0644`, so one writer's same writes give the same bytes; nothing pins them, and an archive another tool makes of the same tree is the same package. An imported package is never distributed, and an authored one travels as one blob per file ([One blob per file](../docs/design/05-protocol-spec.md#packages)).
- **Stored entries only; any other is refused at the open.** An archive that holds an entry of a method other than 0, stored, an encrypted entry, or a second disk, does not open, and its error names the entry and its method ([APPNOTE](https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT) 4.4.4, 4.4.5); an entry the index does not list is refused too, so the rule a writer sees is one rule. Refused: a deflate decoder, as `miniz_oxide`, which an archive of Finder's would still fail on by its top folder, and which most of a package's bytes would not use, as PNG, KTX2, Ogg Vorbis and a glTF's textures are compressed already; a refusal at an entry's first read, which a session meets late, at a map's first file.
- **The central directory is the reader's truth.** Each entry's name, offset and sizes come from the central directory, and its local header gives only the lengths of its name and extra field, to find where its bytes start, as APPNOTE 4.3.6 makes the central directory the archive's record; so a data descriptor after the bytes does no harm, and no CRC is checked, as each read checks its SHA-256. An entry the index does not list is no part of the package and is not read, a directory's entry and Finder's `__MACOSX/` among them, unless the rule above refuses it; two entries of one name, an entry whose bytes pass the end of the archive or reach into the central directory, and a central directory past the end, are refused. Refused: a walk of the local headers, as a streaming reader makes, which an archive with entries removed from its central directory misleads.
- **Each SHA-256 once, in every form.** A source holds the bytes of each SHA-256 of the index at one of its paths at least; the core maps each row to the first path of its hash, in the index's order, that the source holds by its exact name, and a row is `Missing` only when its source holds none, so a directory from `unzip` of an archive is a valid package. The writer writes the first path of each SHA-256 alone, in either form. Refused: a hard link or a copy at each other path, as `PackageWriter` makes now, which `cp -r`, `robocopy` and an archiver turn into full copies, and FAT into a copy at the write; two zip entries on one local header, which Info-ZIP's `unzip` 6.0 refuses as overlapped since its fix of CVE-2019-13232 ([Fifield, A better zip bomb](https://www.bamsoftware.com/hacks/zipbomb/)); a table of aliases beside the index, which holds what two rows of one SHA-256 already say. Takes the **Package** issue of `.notes/ISSUES.md`.
- **The tree is the record of an authored package, the index of a written one.** `index_files` indexes an author's tree, whose duplicates are files the author placed; a written package, which holds each SHA-256 once, is never indexed again from its tree, which would give an index without its duplicate paths.
- **A dependency names a package, not its form.** A manifest's dependency `path` names the place `X`, which is the directory `X` or the archive `X.zip`; a place that holds both is refused, and one that holds neither is `Missing`, both found by the reads themselves, as no code asks whether a path exists. An archive's place, which its dependencies join to, is its path without `.zip`, so `game.zip`'s `../zero-hour` is `zero-hour.zip` beside it. A command line names the exact path of a directory or a `.zip` file. Refused: the form in the manifest, which would change the mode's fingerprint with its form.
- **The open reads the central directory once.** An archive's source holds one `InputRanges` handle and its table of entries in memory, and each read is one positional read of the row's size, as a directory's read is. Refused for now: `mmap`, which `store` does not have and which saves no copy while a read returns a `Vec<u8>`; entries aligned as Android's `zipalign` aligns them, which serve a read with no copy that nothing makes.
- **The zip layout is a problem of `package-format`.** The encode and the decode of zip's records and its CRC-32 are code of `package-format` with no IO, as `quinn-proto` holds QUIC with none; the archive's source and its writer are `package-disk`'s, through `store` ([A package's format is a crate apart from its schemas](../docs/design/02-engine-core.md#modules)). Refused: the `zip` and `rc-zip` crates, a new dependency of which most of the code serves compression and encryption that an archive refuses; `codec`, the gateway of serde's formats, which holds no container.
- **The archive writes durable and whole.** The writer writes `<name>.zip.part`, syncs it once and renames it, so a crash leaves only the part, where a crash in a directory's write leaves part of a package. `store` gains a file written piece by piece and named as it finishes, as `DurableFile::write_with` takes the whole write as one closure, which the importer's run cannot be. Refused: a sync after each file.
- **One writer of two forms.** `PackageWriter` writes a directory or an archive by a `PackageForm` its creation names, so the importer and `ModelWriter` keep one type. Refused: two writer types, which make `ModelWriter` generic over its writer.
- **The import writes archives.** The import writes `game.zip` and `zero-hour.zip`, and `ZeroHour::OUTPUT_VERSION` rises, so a kept import of directories is written again ([A check keeps an unchanged import](../docs/design/09-zero-hour.md#decisions)); the files and the fingerprints are those of now, so the golden file does not move. The test packages stay directories, as authors edit theirs.
- **The store finds archives.** `PackageStore::scan` takes a file `<name>.zip` as a package, as it takes a directory that holds `manifest.toml`, and one whose index does not read is a failure of its own.

## Checks

- An archive of the writer, and the directory `unzip` gives of it, read with one fingerprint and load the same mode on each map.
- A directory and an archive that hold one file of each SHA-256 load; a row whose SHA-256 no held path has is `Missing`, by its path.
- An archive made by Info-ZIP's `zip -0 -r`, kept as a fixture, loads; one by `zip -6` does not open, and its error names the first deflated entry and method 8.
- Each malformed archive is refused with no read past its end and no allocation past its index's sizes: an entry past the end, an entry in the central directory, a central directory past the end, two entries of one name, a comment that holds a false end record.
- The Zero Hour check plays every play from the two archives, with no golden moved.

## Steps

### Batch 1: each SHA-256 once

Touches `package-format`, `package-disk` and `store`: the rule every form of a package keeps, first, as the archive builds on it.

- **PA1.** The open takes a row only by its own path: `PackageFiles::read_indexed` gives `check_names` every path of the index, and `PackageReader` reads each row at its own path. The source answers which paths it holds by exact name, which `Listings::hold` already answers for a directory and `TreeDir` from its map, and the reader maps each row to the first held path of its SHA-256 (Each SHA-256 once, in every form). Builds "A name is resolved once": the map is made at the open, and no read searches. Takes the **Package** issue.
- **PA2.** `PackageWriter::write` makes a hard link or a copy at the second path of a SHA-256; it keeps the row alone. `OutputFile::link_new` loses its last caller and goes, with its test.

### Batch 2: the zip layout

Touches `package-format` alone: the records as bytes, which the next batch's reader and writer take, tested with no file.

- **PA3.** No code decodes a zip: the end record, its zip64 record and locator, the central directory's entries and a local header's lengths, from bytes, each refusal of the reader's rule its own case of an error enum (The central directory is the reader's truth; Stored entries only; any other is refused at the open).
- **PA4.** No code encodes one: a stored entry's local header, its central entry, the zip64 records past 4 GiB of offsets, the end record and CRC-32, as the writer's fixed fields give them (The archive's bytes are not identity). A round trip of encode and decode checks the pair; the Info-ZIP fixtures check the decode against another writer.

### Batch 3: archives on disk

Touches `store` and `package-disk`, and `package`'s `PackageRef`: the archive as a source and a place, and its writer.

- **PA5.** `store` has no file written piece by piece and named as it finishes (The archive writes durable and whole).
- **PA6.** No source reads an archive: `PackageArchive` opens `InputRanges`, reads the end record and the central directory once, and reads each held path by one positional read (The open reads the central directory once).
- **PA7.** A place is a directory alone: `PackagePlace::dir` names a directory, `PackageRef::Dir` an error's package, and `join` a directory's dependency. A place resolves to a directory or an archive, both found by their reads, and joins from an archive's path without `.zip` (A dependency names a package, not its form).
- **PA8.** `PackageWriter` writes only a directory: it writes a `PackageForm` (One writer of two forms).
- **PA9.** `PackageStore::scan` finds only directories (The store finds archives).

### Batch 4: the import and the programs

Touches `import`, `zero-hour-check`, `client`, `server`, `verifier` and `harness`, and the design documents: the archive used, and the slice folded.

- **PA10.** The import writes directories, and the check reads and removes them (The import writes archives). `cargo drive run zero-hour` passes with no golden moved.
- **PA11.** The client's and the server's `--mode`, the verifier's store and the bench's `PackageDir::new` take a directory alone; each takes a place of either form.
- **PA12.** The slice's decisions are in this file alone: they move into `05-protocol-spec.md` Packages, `03-game-scripting.md` Game package and `02-engine-core.md` Modules, the fact of Info-ZIP's refusal of overlapped entries goes to `docs/research/`, dated, and this file is deleted. The slice ends when the client plays a skirmish from the two archives.
