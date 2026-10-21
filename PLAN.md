# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## Slice: Unit picking

A player points at a unit beside or in front of a building and the unit takes the hover and the click, and a small unit is as easy to point at as a large one, as the mode's data chooses ([proposal](docs/design/12-unit-picking.md)). Each batch is one commit, ends with `cargo drive gate` passing, and stops for review.

### 1. The mode's picking options (M)

`client-files`, `client`, `import`, the test packages, 02, 09. It replaces the picking backend's defaults, one tier and a least radius of 0, with the mode's choice; last: it ends the slice.

- **The file.** `client/picking.toml`, a `ClientFile` of TOML: `tiers`, a list of filters, and `least_radius_px`, a number at least 0, each its default when absent; the client resolves the filters once as it loads the mode, against the match's tags, refusing a tag the match lacks, and ranks a unit by its type's tags and its team's relation to the player.
- **Zero Hour's choice.** The import writes `tiers = ["all:!structure"]` and `least_radius_px = 10`; the MOBA test packages write `tiers = ["enemies:avatar", "enemies"]`, as a hero's player points at heroes first.
- **The fold.** The decisions go to 02's Client and 09, the facts stay in research/unit-picking.md, and the proposal is deleted.
- **Check:** tests of the file's round trip and its defaults, of a refused tag, of the tiers a file gives, and of the import's file; `cargo drive run zero-hour`, on request, moves its fingerprints; the user plays Tournament Desert in the client and points at a dozer beside a barracks and at infantry far off.

## Foundations
