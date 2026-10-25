# Plan

The open steps, in the order they are done, and the foundations. The rules are in [AGENTS.md](AGENTS.md#workflow).

## A counted fog

The Vision stage costs what changed since the tick before, not each seer's whole sight again: a base that stands still costs nothing, as in Zero Hour, StarCraft II and Warcraft III. Two batches: geometry and navigation first, as the fog counts by their diff of runs; then vision alone. No golden moves: the fog's bits stay a pure function of the seers and the reveals, and the fog is a cache that no hash or snapshot holds. The spike is `../campfire-perf/fog`, its numbers in [the perf journal](docs/research/perf-journal.md#session-of-2026-10-25).

### F1. The runs that changed (geometry's grid)

First, as the counted fog needs the diff that crowd costs keep to themselves.

- [ ] `CrowdCosts::recount` finds the cells a unit's footprint lost and gained, row by row, in a loop of its own (`crowd_costs/mod.rs:261`), and the counted fog needs the same → `Grid` gives the runs that two sets of runs, one a row and rows in order as `spans_within` and `spans_closer` give them, lose and gain, each a run with whether it was lost or gained, and `recount` takes its cells from it. Builds: each operation has one home, the grid's runs the grid's.
- Checks: a geometry test of two discs, the same, moved within their rows, moved a row up, grown and apart, whose lost and gained cells are exactly the cells of one disc and not the other; `counts_kept_through_moves_are_those_counted_afresh` passes unchanged.

### F2. Counted sight (vision's fog)

- [ ] Each tick clears the words the tick before set and sets every seer's runs again (`sight_maps/mod.rs:41`, `:59`), so a seer that stands still pays its whole sight each tick → each bitmap, of a group's sight or its detection, has beside it the count of the group's sights on each cell, a `u16`, and a cell's bit is set while its count is above 0; a look counts a run up or down, but the cells its brush hides. The marks of the words set, the clear and `Bitmap` go. Builds: a pass visits what changed.
- [ ] `Fog::sight` reveals each seer's runs again (`fog.rs:83`) → the fog keeps each seer's look by its slot: its place, range, group, detection, and the brush it stands in; and the slots that look, in a list. A seer of the same look costs a compare; one that moved, of the same group, detection and brush, counts only the runs F1 gives from its old place's runs and its new's, both from the grid, as crowd costs take theirs; any other change takes back its old sight whole and counts its new. When the stage's seers are done, each slot that did not look takes its sight back, as a unit that died or lost its sight. A reveal counts as the stage runs and is taken back as the next stage begins. `SightCache<Sighting>` goes, and the cache serves the box covers alone, named for them. Builds: a pass visits what changed.
- [ ] `BrushMap::hidden_from` takes a cell (`brush_map.rs:53`) → the brush of a cell, and the cells a seer in a brush does not reveal, as a look keeps its brush, not its cell. Builds: a name is resolved once.
- [ ] `Seeing::fill` ends the seers' looks before the reveals (`seeing.rs:118`), and the bench `fog` does the same.
- [ ] `fog.rs` gains its tests, so it becomes `fog/{mod.rs,tests.rs}`.
- [ ] `vision.md`'s "A stage that costs what units see" becomes "A stage that costs what changed": each cell counts each group's sights on it, a seer that stood still costs nothing, and a moved one counts the cells its rows lost and gained, as Zero Hour's `PartitionCell` counts its lookers in a `Short` (`Display.h`, `m_currentShroud`), so a count is a `u16`; refused: the bitmaps cleared and set again each tick, whose cost grows with every seer however still; Zero Hour's look of the whole circle again on each move of a cell (`Object::handleShroud`), which pays a moved seer's whole sight twice; each seer's runs kept between ticks, a buffer per seer where the grid gives them again from the look's key.
- Checks: a test of 200 random ticks of 40 seers over three groups, two brushes, detection and reveals, whose every cell's sight and detection equal a fog built again each tick, as the spike's; two seers of one group on one cell, one leaves and the cell stays seen, the other leaves and it is not; a seer that steps into a brush stops revealing the other brushes' cells, and one that leaves it reveals them again; a relations change and a restore fill the fog as now. `local` and `mac`: `fog/still`, `battle/crowded_vision` and `server_stage/mean_3v3_vision` gain as the spike's did (−81 %, −64 %, −33 % on `local`); `fog/sight`, every seer moved, and `server_stage/worst_3v3_vision` lose nothing beyond their floors; `cargo drive run zero-hour` holds its golden.

## Foundations
