# campfire-math

The numbers every crate of the sim shares: the fixed-point `Num`, vectors, roots, trig, the rounding of a quotient, and the random sequences. Every result is the same bits on every machine. The design is [Determinism core](../../../../docs/design/07-determinism-core.md): its decisions on `Num`, the one `i128` step, exactness, roots and references hold here, and the rule below adds what they leave to this crate; the repository's `AGENTS.md` comes first.

## The rule

- **One scalar.** A value of the sim is a `Num`, fixed point in an `i64`. A new kind of quantity is a `Num` or a type over it, never a new number format.
- **A rounding is a stated bound.** Each operation that rounds names, in its doc comment, how far its result may lie from the true one, in bits of a `Num`. A compound value is a chain of such steps, and its bound is their sum.
- **A tolerance says why it is there.** A slack in a test covers a named rounding and lives beside the value it covers. A step whose result a later test must see one way rounds toward that way: a push rounds away from what it leaves.
- **No state.** The crate holds values and pure operations; the sim's lifecycle, as its running tick, lives in `sim`.

## Platforms and speed

- A path for one platform lives behind one entity whose API is the same on every platform, and gives the same bits as the plain form, which a test checks on every machine.
- Such a path, or a lane of four values, stays only while a bench shows it gains on the plain form; when it does not, the plain form takes its place.

## Tests and benches

- Every operation has hand-computed cases at its ties, limits and sign changes, and `proptest` differential tests against an exact reference.
- Each operation has an atomic bench in a `bench.rs` beside it, `atomic/<module>/<case>` over 4,096 drawn inputs, one case for each path whose cost differs, as [Benches](../../../../docs/design/02-engine-core.md#benches) says.
