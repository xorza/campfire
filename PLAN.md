# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Integer roots

The design is [Integer roots](docs/design/14-integer-roots.md): one exact floor root in `math`, in a `u64` and a `u128` path, which `Num`'s roots and the two callers of `u128::isqrt` take. Each step measures its cases before and after its change, in one run on one core, and ends with the check chain passing for its crate.

1. **M1. The root** (math; R1, R2, R3, R5, R6): `FloorRoot::floor_root` for `u128`; `Num::from_root_of_bits` on it; the case `root/floor`, and the group in design 13's B2 and suite. Check: the hand-computed cases and the differential test against `u128::isqrt`; the `num` and `vec3` golden digests unchanged; `root/floor`, `num/sqrt` and `vec3/distance` before and after, in design 14.
2. **M2. The grid's spans** (core; R4): `Grid::spans_within` takes `floor_root`. Check: the grid's span tests and the proving match's goldens unchanged; `fog/sight` and `server_tick/mean_3v3` before and after, and vision's Cost section.
3. **M3. The collider** (navigation; R4): `Collider::part` takes `floor_root`. Check: the collider's tests and the proving match's goldens unchanged; `collision/crowded` and `collision/spread` before and after, and navigation's Cost section.
