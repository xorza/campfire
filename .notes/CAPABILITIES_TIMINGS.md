# Capabilities timings

The `capabilities` crate's atomic benches of the grid, the body index, the body grid and the box
tests, in µs for each run of the bench, its 4,096 inputs (`body_grid/rebuild`: 1,000 bodies;
`body_index/update`: one update). Ryzen is a Ryzen 7 6800U (x86-64-v3), pinned to one core; M2 is
an Apple M2. Each figure is the best of three runs, the base and the change run in turns. The base
is `671080ba`.

## Current

| Bench | Ryzen base | Ryzen now | Change | M2 base | M2 now | Change |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `atomic/body_box/overlap` | 31.2 | 31.3 | +1 % | 22.7 | 22.9 | +1 % |
| `atomic/body_box/reach` | 143.9 | 23.9 | −83 % | 119.4 | 41.2 | −66 % |
| `atomic/body_grid/rebuild` | 24.7 | 21.6 | −13 % | 24.3 | 20.7 | −15 % |
| `atomic/body_grid/visit_near` | 1,640.6 | 1,309.7 | −20 % | 1,342.0 | 1,096.4 | −18 % |
| `atomic/body_index/blocks` | 1,956.3 | 635.1 | −68 % | 1,756.3 | 647.7 | −63 % |
| `atomic/body_index/near` | 293.0 | 296.8 | +1 % | 359.6 | 354.2 | −1 % |
| `atomic/body_index/update` | 5.84 | 5.83 | 0 % | 4.76 | 4.86 | +2 % |
| `atomic/grid/box_spans_closer` | 24,288 | 5,109 | −79 % | 21,795 | 5,514 | −75 % |
| `atomic/grid/spans_closer` | 291.3 | 290.0 | 0 % | 316.8 | 316.4 | 0 % |
| `atomic/shape/comes_within_box` | 468.0 | 29.4 | −94 % | 387.6 | 36.3 | −91 % |
| `atomic/shape/comes_within_circle` | 39.6 | 18.6 | −53 % | 39.1 | 29.2 | −25 % |

`body_index/near` and `body_index/update` run the base's code. The M2's figures move by up to 7 %
from run to run of one binary.

The integration benches that run these paths do not move: `integration/collision/*`,
`gather/two_fields`, `pathing_grid/enter` and `route_planner/*` are within ±2 % on the Ryzen and
±3 % on the M2, in both directions. Their time lies elsewhere.

## Kept

| Change | Where it gains |
| --- | --- |
| `Grid::runs`: each row's run searched from the row before's boundaries, a convex shape's, so the cells between them take no test; each cell's centre from its row and column, with no division | `grid/box_spans_closer` |
| `Closer`: a box's test of the points closer than a reach, its corners, edges, squared lengths and each edge's bound made once for a body; a cross product below 2⁶⁴ squared on `u128`; the first close feature ends the search | `grid/box_spans_closer`, `body_index/blocks` |
| `BodyBox::nearest`: past the reach when a point lies past the extent and the reach along an axis; else each feature measured against the reach alone, not against each other by 384-bit products | `body_box/reach` |
| `BodyBox::comes_within`: whether a path comes closer than a reach, with no share and no fraction, by separating axes for the path's entry | `shape/comes_within_box`, `body_index/blocks` |
| `Shape::comes_within`: a path whose span along an axis lies a reach beyond the shape's is not closer, with no exact test | `shape/comes_within_*`, `body_index/blocks` |
| `BodyGrid`: sorted by one 128-bit key of 32-byte orders, the bodies gathered once; searched over a dense array of 64-bit cell keys | `body_grid/rebuild`, `body_grid/visit_near` |
| `Grid`'s box spans: the floor of halves on `i64`, not the `i128` library call | `grid/box_spans_closer` |
| `U256::product`: wrapping sums, which cannot pass `u128` | the box tests' 256-bit products |

## Tried and dropped

| Change | Ryzen | M2 | Why dropped |
| --- | ---: | ---: | --- |
| `CellLevels::index` by a float reciprocal and one exact step, not a 64-bit division | `body_index/near` +20 % | +15 % | A 64-bit division is fast on both: 10 to 18 cycles on Zen 3, fewer on the M2 |
| `BodyIndex` searched over a dense array of bucket keys | `body_index/near` +3 % | +3 %, `update` +7 % | A row's run holds a few entries: the search is short, and the array costs a pass at each update |
| `BodyGrid`'s run of a row scanned to its end, not searched | `body_grid/visit_near` −5 % against −20 % | +17 % | A row's run of cells holds many bodies |
| `Closer`'s edge bounds made on demand | `grid/box_spans_closer` −75 % against −79 % | −73 % against −75 % | The grid tests many points of one box |
| `BodyBox::overlaps` rejecting boxes whose extents lie apart | `body_box/overlap` −12 % | +148 % | On the M2 the branch costs more than the separating axes it skips |
| `Closer::holds` by `edge_against`'s orders | `grid/box_spans_closer` +26 % | +11 % | A compare taken as an order and tested costs more than one compare |
| `spans_closer` | — | — | Not changed: four rows a step in lanes, and no cost stands out in its profile |
