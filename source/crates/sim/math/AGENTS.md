# campfire-math

The exact numbers every crate of the sim shares: the fixed-point `Num`, vectors, roots, trig, the rounding of a quotient, and the random sequences. Every result is the same bits on every machine. The design is [Determinism core](../../../../docs/design/09-determinism-core.md) and [Math in 128 bits](../../../../docs/design/16-math-128-bits.md); the repository's `AGENTS.md` comes first.

## The rule

- **One scalar.** A value of the sim is a `Num`, fixed point in an `i64`. A new kind of quantity is a `Num` or a type over it, never a new number format.
- **One step, 128 bits.** An operation computes its result from its inputs in one `i128` and rounds it once: to nearest, ties to even, unless a rule names its `Rounding`. No type of the crate is wider than 128 bits. An operation whose exact form would pass 128 bits rounds an input first, and says by how much.
- **Exact where it is free.** A comparison whose exact form fits 128 bits stays exact: a squared distance between two points against a squared reach, a sign, an order of integers. Precision is given up only where exactness would cost a wider type.
- **A rounding is a stated bound.** Each operation that rounds names, in its doc comment, how far its result may lie from the true one, in bits of a `Num`. A compound value is a chain of such steps, and its bound is their sum.
- **A tolerance says why it is there.** A slack in a test covers a named rounding and lives beside the value it covers. A step whose result a later test must see one way rounds toward that way: a push rounds away from what it leaves.
- **No float in a result.** A float may give only a first estimate that integer steps correct, so the result never depends on it. No result, no state and no hash holds a float.
- **No state.** The crate holds values and pure operations; the sim's lifecycle, as its running tick, lives in `sim`.

## Platforms and speed

- A path for one platform lives behind one entity whose API is the same on every platform, and gives the same bits as the plain form, which a test checks on every machine.
- Such a path, or a lane of four values, stays only while a bench shows it gains on the plain form; when it does not, the plain form takes its place.

## Tests and benches

- Every operation has hand-computed cases at its ties, limits and sign changes, and `proptest` differential tests against an exact reference. A reference may be wider than 128 bits; it lives in the test that needs it, never in the crate's API.
- Each operation has an atomic bench in a `bench.rs` beside it, `atomic/<module>/<case>` over 4,096 drawn inputs, one case for each path whose cost differs, as [Benches](../../../../docs/design/02-engine-core.md#benches) says.
