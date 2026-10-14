use std::hint::black_box;
use std::num::NonZeroU64;
use std::rc::Rc;

use criterion::{Criterion, Throughput};
use rhai::INT;

use crate::script_host::ScriptHost;
use crate::script_host::budget::Budget;

/// The calls of a case: one for each unit of a kernel's scene, as a tick of 1,000 AIs makes.
const CALLS: usize = 1000;
/// The 3v3's limit per call.
const PER_CALL: NonZeroU64 = NonZeroU64::new(20_000).unwrap();

/// A value of a type the host registers, of a unit handle's size: its stable id, its row and the
/// view it shares.
#[derive(Debug, Clone)]
#[expect(
    dead_code,
    reason = "its fields give it a handle's size, which no case reads"
)]
struct Handle {
    id: u64,
    row: usize,
    view: Rc<()>,
}

/// The script host's own cost of a hook call, `CALLS` times over: an empty hook of two arguments,
/// `call`, what each AI think pays before its script runs; the same hook given a handle of a
/// registered type for its unit, `handle`, which Rhai boxes twice; a hook that makes one call to a
/// function the host registered, `native`, as each `ctx` query is; and one whose call returns a
/// handle, `native_handle`, as each query that gives a unit does.
pub(crate) fn script(c: &mut Criterion) {
    let mut host = ScriptHost::new(PER_CALL);
    let view = Rc::new(());
    let made = Rc::clone(&view);
    host.engine_mut()
        .register_fn("probe", |value: INT| value + 1)
        .register_fn("make", move |value: INT| Handle {
            id: value.unsigned_abs(),
            row: 0,
            view: Rc::clone(&made),
        });
    let script = host
        .compile(
            "fn empty(ctx, unit) {}\n\
             fn native(ctx, unit) { probe(unit) }\n\
             fn native_handle(ctx, unit) { make(unit) }",
        )
        .expect("the bench's script compiles");
    let mut budget = Budget::new(u64::MAX);
    let ctx: INT = 0;

    let mut group = c.benchmark_group("atomic/script");
    group.throughput(Throughput::Elements(CALLS as u64));
    let cases = [
        ("call", "empty"),
        ("native", "native"),
        ("native_handle", "native_handle"),
    ];
    for (case, hook) in cases {
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
    group.bench_function("handle", |b| {
        b.iter(|| {
            for unit in 0..CALLS {
                let handle = Handle {
                    id: u64::try_from(unit).expect("a call's index fits"),
                    row: unit,
                    view: Rc::clone(&view),
                };
                let called = host
                    .call(&mut budget, script, "empty", (ctx, handle))
                    .expect("the bench's hooks run");
                black_box(&called);
            }
        });
    });
    group.finish();
}
