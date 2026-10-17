# Script view reads

What a read of the script view costs, measured 2026-10-17.

## Machine and method

- **Machine:** Intel Core i9-13980HX, Linux, x86-64-v3, a laptop on mains power, with other work running.
- **A run:** `./drive.py bench -- integration/script_view`, criterion's defaults. The scene is the MOBA 3v3 at its start with 1,000 creeps more. `read` follows a move of every unit, as a tick's first batch finds them; `reread` a move of one unit in 100, as a later batch of the same tick does.

## Results

| Case | A walk of every unit, at `75cdf2589` | Only the units touched, from the next commit |
| --- | ---: | ---: |
| `integration/script_view/read` | 97.2 µs | 82.1 µs |
| `integration/script_view/reread` | 65.8 µs | 35.1 µs |

- With a walk of every unit, a read with one unit in a hundred changed cost 68 % of a read with every unit changed: the walk, not the units filled, set the cost.
- A read that sorts the touched units and looks up each one's entity in the B-tree index cost 122.4 µs for `read`, 26 % more than the walk; past half the rows marked, a walk of the index in id order costs less.
