# Plan questions

## D1. A segment of no road type

- **Item.** D1's second step, and `roads.md`'s decision *A road's type is `Roads.ini`'s*: "a segment of a template no type names, 3 in the install, draws nothing and is a loss of the import, as the game draws it with its first type only in a build of test assets".
- **What the code says.** `W3DRoadBuffer::addMapObjects` sets `m_uniqueID = 1`, `m_scale = DEFAULT_ROAD_SCALE` (8) and `m_widthInTexture = 1` before it looks the template up, in every build; only the fallback after it is under `LOAD_TEST_ASSETS`. The ids count roads and bridges in the order `Roads.ini` reads them from 1, and its first block is `Road TwoLane`, so the release game draws such a segment as `TwoLane`'s texture, 8 units wide, chained with the `TwoLane` segments it meets.
- **Options.**
  - Keep the decision: the segment draws nothing and is a loss of cause `NoType`. What D1 implements.
  - Follow the release game: the segment is of the type of id 1, 8 wide and 1 of its texture across, and no loss. It needs the ids of roads and bridges in read order, a field of `RoadsIni`.
- **Recommendation.** Follow the release game, and correct the decision's fact in `roads.md`: the slice's aim is the roads as the game draws them, and the census counts 3 such segments.
- **Blocked.** Nothing: D1 to D4 hold either way; the change is local to `MapRoads::of` and `RoadsIni`.
