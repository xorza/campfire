# Plan — next steps

The concrete steps of the current stages of the [roadmap](ROADMAP.md), in order, open items only: remove a step when it is done, and a section when it is empty. Each step ends with the check chain passing for the crates it touches, and a stop for review. A later stage's steps come here when it starts, after its design step ([Workflow](AGENTS.md#workflow)).

## Integer roots

The design is [Integer roots](docs/design/14-integer-roots.md): one exact floor root in `math`, in a `u64` and a `u128` path, which `Num`'s roots and the two callers of `u128::isqrt` take. Each step measures its cases before and after its change, in one run on one core, and ends with the check chain passing for its crate.

1. **M3. The collider** (navigation; R4): `Collider::part` takes `floor_root`. Check: the collider's tests and the proving match's goldens unchanged; `collision/crowded` and `collision/spread` before and after, and navigation's Cost section.
