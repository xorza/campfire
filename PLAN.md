# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## 1. A readable package index

`package`: `file_index`; `package-disk`; `.gitattributes`; the workspace's packages; `checks/zero-hour-check`.

### I1. The index as a line manifest

(`package/src/file_index/`; design 05, The index.) `package.index` is postcard after the tag `campfire/package-index/v1`, which no reader or diff shows; design 05 now gives it as text, `campfire/package-index/v2` then `<SHA-256> <size> <path>` lines. `FileIndex`'s encode writes that text, and its decode reads it and refuses every other spelling of the same rows: a CR, uppercase hex, a leading zero, two spaces, a missing last LF, rows out of order, the old tag. `.gitattributes` gives `package.index` as text of LF, not binary. Each workspace package's index is written again, the goldens that name a fingerprint blessed, and the Zero Hour check's fingerprints pinned again on Linux and on the Mac.
