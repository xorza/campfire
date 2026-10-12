# Plan questions

## C3. Positions in fractions of a meter

**Item.** C3 removes the tests' copies of `at`, `ground`, `half` and `point`. The copies in whole meters are gone. Six remain, and each converts a unit smaller than a meter, not a copy of the whole-meter helper:

| Helper | Unit | Calls |
|---|---|---|
| `half(x, z)` in `production/tests/{orders, construction, gathering}.rs` | half meters | about 70 |
| `point(x, z)` in `navigation/tests/groups.rs` | half meters | 9 |
| `at(x, z)` in `navigation/route_planner/tests.rs` | quarter meters | 20 |
| `at(x, z)` in `navigation/steering.rs`'s tests | eighth meters | 6 |

**Why it needs a call.** D7 sends a fractional position through `TryFrom<Vec3>`, on the premise that such positions are rare. Here they are a test's whole grid. Each call would grow from `half(1, 8)` to `Position::try_from(Vec3::from((1, 0, 8)) * Num::HALF).unwrap()`.

| Option | What it does | Cost |
|---|---|---|
| A. Keep them, named by their unit (recommended) | Each stays a private helper of its test file, named for its unit: `halves(x, z)`, `quarters(x, z)`, `eighths(x, z)`. D7 and C3 say that a helper that converts a unit stays. | Four small helpers stay. |
| B. One shared constructor | A test-only `Position::in_fractions((x, y, z), denominator)` in `sim`'s internals replaces all four. | A new test API on `Position`, with an exponent or divisor parameter at every call. |
| C. `TryFrom` at each call | Every call goes through `Vec3` scaled by the fraction, then `try_from` and `unwrap`. | About 105 long calls. |

**Blocked.** Only these six helpers. The rest of C3 is done.
