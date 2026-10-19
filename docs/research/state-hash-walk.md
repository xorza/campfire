# State hash walk

What a query of each state type's holders, in place of a walk of every entity, changes in the state hash and the snapshot, measured 2026-10-18.

## Machine and method

- **Machine:** Intel Core i9-13980HX, 32 threads, Linux 7.2, a laptop on mains power, with CI running remotely and no other build.
- **A run:** `./drive.py bench -- 'integration/state_hash|integration/snapshot/all'`, criterion's defaults, over the bench's world of 1,000 units of eight component types. The walk, at `581a66b5a`, is the baseline: each type walks the entity index in stable-id order and reads its value of each entity. The query form reads each type's holders by a query that admits the disabled and the unpredicted, collects them with their ids, and sorts them by id before it encodes them.

## Results

| Case | The walk | The query | Change |
| --- | ---: | ---: | ---: |
| `integration/state_hash/all` | 101.0 µs | 105.8 µs | +4.8 % |
| `integration/state_hash/by_type` | 101.0 µs | 105.8 µs | +5.0 % |
| `integration/snapshot/all` | 99.9 µs | 105.8 µs | +5.9 % |

- The query form is slower in each case: its collection and sort cost more than the walk's lookups.
- About 12.5 ns of each value's cost, 100 µs over 8,000 values, is its encoding and hashing, which neither form changes.
