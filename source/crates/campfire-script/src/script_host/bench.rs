use std::hint::black_box;

use criterion::{Criterion, Throughput};
use rhai::INT;

use crate::script_host::ScriptHost;
use crate::script_host::budget::Budget;

/// The calls of a case: one for each unit of a kernel's scene, as a tick of 1,000 AIs makes.
const CALLS: usize = 1000;
/// The 3v3's limit per call.
const PER_CALL: u64 = 20_000;

/// The script host's own cost of a hook call, `CALLS` times over: an empty hook of two arguments,
/// `call`, what each AI think pays before its script runs; and a hook that makes one call to a
/// function the host registered, `native`, as each `ctx` query is.
pub(crate) fn script(c: &mut Criterion) {
    let mut host = ScriptHost::new(PER_CALL);
    host.engine_mut()
        .register_fn("probe", |value: INT| value + 1);
    let script = host
        .compile("fn empty(ctx, unit) {}\nfn native(ctx, unit) { probe(unit) }")
        .expect("the bench's script compiles");
    let mut budget = Budget::new(u64::MAX);
    let ctx: INT = 0;

    let mut group = c.benchmark_group("script");
    group.throughput(Throughput::Elements(CALLS as u64));
    for (case, hook) in [("call", "empty"), ("native", "native")] {
        group.bench_function(case, |b| {
            b.iter(|| {
                for unit in 0..CALLS {
                    let unit = INT::try_from(unit).expect("a call's index fits");
                    let called = host
                        .call(&mut budget, script, hook, (ctx, unit))
                        .expect("the bench's hooks run");
                    black_box(&called);
                }
            });
        });
    }
    group.finish();
}
