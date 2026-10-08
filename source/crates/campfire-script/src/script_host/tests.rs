use std::cell::RefCell;

use campfire_math::Num;
use rhai::{EvalAltResult, INT, ImmutableString};

use super::*;
use crate::script_host::error::{NumError, Raised};

const PER_CALL: NonZeroU64 = NonZeroU64::new(1000).unwrap();

fn host() -> ScriptHost {
    ScriptHost::new(PER_CALL)
}

/// A budget no call spends.
fn ample() -> Budget {
    Budget::new(u64::MAX)
}

/// Runs `body` as the function `f` of a new script.
fn run(host: &mut ScriptHost, body: &str) -> Result<Dynamic, ScriptError> {
    let script = host.compile(&format!("fn f() {{ {body} }}"))?;
    host.call(&mut ample(), script, "f", ())
}

fn num(host: &mut ScriptHost, body: &str) -> Num {
    run(host, body).unwrap().cast::<Num>()
}

const HALF: i64 = 1 << 23;

#[test]
fn num_arithmetic_is_exact_and_checked() {
    let mut host = host();
    // 3 / 2 + 1 = 2.5 exactly; 1 / 3 rounds to nearest: 5 592 405.33 → 5 592 405 raw.
    assert_eq!(num(&mut host, "num(3) / 2 + 1"), Num::from_bits(5 * HALF));
    assert_eq!(num(&mut host, "num(1) / 3"), Num::from_bits(5_592_405));
    // An integer becomes a `Num` on either side of an operator and a comparison.
    assert_eq!(num(&mut host, "3 * num(2) - 1"), Num::from_int(5).unwrap());
    assert_eq!(num(&mut host, "-(num(7) - 10)"), Num::from_int(3).unwrap());
    assert!(
        run(&mut host, "num(5) > 4 && 4 < num(5) && num(2) == 2")
            .unwrap()
            .as_bool()
            .unwrap()
    );
    // `round` is half away from zero; `floor` and `ceil` return integers too.
    let integers = [
        ("round(num(-5) / 2)", -3),
        ("round(num(5) / 2)", 3),
        ("floor(num(-5) / 2)", -3),
        ("ceil(num(-5) / 2)", -2),
    ];
    for (body, expected) in integers {
        assert_eq!(
            run(&mut host, body).unwrap().as_int().unwrap(),
            expected,
            "{body}"
        );
    }
    for (body, expected) in [
        ("min(num(2), num(3))", 2),
        ("max(num(2), 3)", 3),
        ("min(-1, num(2))", -1),
        ("num(-4).max(0)", 0),
    ] {
        assert_eq!(
            num(&mut host, body),
            Num::from_int(expected).unwrap(),
            "{body}"
        );
    }
    assert_eq!(
        num(&mut host, "clamp(num(9), num(0), num(4))"),
        Num::from_int(4).unwrap()
    );

    // 2²⁰ × 2²⁰ = 2⁴⁰ is past 40.24's range, 1 / 0 has no value, and 2³⁹ is no `Num`.
    for (body, expected) in [
        ("num(1 << 20) * num(1 << 20)", NumError::Overflow),
        ("num(1) / 0", NumError::Overflow),
        ("num(1 << 39)", NumError::IntegerBeyondNum),
        ("clamp(num(1), num(4), num(0))", NumError::ClampBounds),
    ] {
        let Err(ScriptError::Raised(raised)) = run(&mut host, body) else {
            panic!("{body} raises");
        };
        assert_eq!(raised.get::<NumError>(), Some(expected), "{body}");
        assert_eq!(raised.get::<INT>(), None, "{body}");
    }
    // A value raised in a function the hook calls comes out of Rhai's chain of calls, as raised;
    // any other failure there keeps the chain as Rhai gave it.
    let script = host
        .compile("fn g() { num(1 << 39) } fn h() { nope() } fn f() { g() } fn e() { h() }")
        .unwrap();
    let Err(ScriptError::Raised(raised)) = host.call(&mut ample(), script, "f", ()) else {
        panic!("g raises");
    };
    assert_eq!(raised.get::<NumError>(), Some(NumError::IntegerBeyondNum));
    let Err(ScriptError::Runtime(error)) = host.call(&mut ample(), script, "e", ()) else {
        panic!("h fails");
    };
    assert!(
        matches!(*error, EvalAltResult::ErrorInFunctionCall(..)),
        "{error:?}"
    );
    assert!(
        matches!(
            error.unwrap_inner(),
            EvalAltResult::ErrorFunctionNotFound(..)
        ),
        "{error:?}"
    );
    // Integer overflow fails too: Rhai checks it.
    let max = INT::MAX;
    assert!(matches!(
        run(&mut host, &format!("{max} + 1")),
        Err(ScriptError::Runtime(_))
    ));
}

#[test]
fn only_what_scripts_need_is_there() {
    let mut host = host();
    // No floats: a decimal literal does not parse.
    assert!(matches!(
        host.compile("fn f() { 1.5 }"),
        Err(ScriptError::Compile(_))
    ));
    // No `eval`, no imports, no `sleep`, no clock.
    assert!(matches!(
        host.compile(r#"fn f() { eval("1") }"#),
        Err(ScriptError::Compile(_))
    ));
    for (body, error) in [
        (r#"import "x" as x; 1"#, "Too many modules imported"),
        (
            "sleep(1)",
            "Function not found: sleep (i64) (line 1, position 10)",
        ),
        (
            "timestamp()",
            "Function not found: timestamp () (line 1, position 10)",
        ),
    ] {
        let failed = run(&mut host, body).err();
        let pinned =
            matches!(&failed, Some(ScriptError::Runtime(text)) if text.to_string() == error);
        assert!(pinned, "{body}: {failed:?}");
    }
    // What scripts do need: loops over arrays and ranges, maps, strings, and `print`, which
    // writes only in debug builds.
    let body = r#"let total = 0; for x in [1, 2, 3] { total += x; } for i in 0..4 { total += i; }
        let m = #{ a: 5 }; print("quiet");
        if "enemies" == "enemies" { total + m.a } else { 0 }"#;
    assert_eq!(run(&mut host, body).unwrap().as_int().unwrap(), 17);
    assert_eq!(hashing::get_hashing_seed(), &Some(HASHING_SEED));

    // A script's functions, each by its name and its number of parameters, and its place.
    let script = host
        .compile("fn on_think(ctx, unit) { 1 } fn helper() {}")
        .unwrap();
    let mut functions: Vec<_> = host.functions(script).collect();
    functions.sort_unstable();
    assert_eq!(functions, [("helper", 0), ("on_think", 2)]);
    assert_eq!(script.index(), host.compiled() - 1);
    assert_eq!(ScriptId::nth(script.index()), script);
}

#[test]
fn calls_fail_at_their_limits_the_same_way_in_every_build() {
    let mut host = host();
    let spin = host.compile("fn spin() { loop {} }").unwrap();
    let mut budget = Budget::new(1500);
    // A call runs its 1000 operations; the 1001st, one past the limit, fails it.
    assert!(matches!(
        host.call(&mut budget, spin, "spin", ()),
        Err(ScriptError::CallLimit)
    ));
    assert_eq!(budget.left(), 500);
    // The budget has 500 left: the next call is ended at its 501st operation, which the
    // progress callback sees before it ends the call, so the budget is spent.
    assert!(matches!(
        host.call(&mut budget, spin, "spin", ()),
        Err(ScriptError::TickBudget)
    ));
    assert_eq!(budget.left(), 0);
    // With the budget spent, a call fails before it runs.
    let one = host.compile("fn one() { 1 }").unwrap();
    assert!(matches!(
        host.call(&mut budget, one, "one", ()),
        Err(ScriptError::TickBudget)
    ));
    // Another budget is untouched by this one's calls.
    let mut other = Budget::new(1500);
    assert_eq!(
        host.call(&mut other, one, "one", ()).unwrap().as_int(),
        Ok(1)
    );
    // `one` costs Rhai 3 operations, which the budget counts whole.
    assert_eq!(other.left(), 1497);

    // The call depth is the engine's 32 in every build: Rhai's default is 8 in a debug build
    // and 64 in a release one. `down(32)` calls down 32 times below the first call and passes;
    // `down(33)` goes one deeper and fails.
    let down = host
        .compile("fn down(n) { if n > 0 { down(n - 1) } else { 0 } }")
        .unwrap();
    assert_eq!(
        host.call(&mut ample(), down, "down", (32_i64,))
            .unwrap()
            .as_int(),
        Ok(0)
    );
    let deeper = host.call(&mut ample(), down, "down", (33_i64,)).err();
    let overflow =
        matches!(&deeper, Some(ScriptError::Runtime(text)) if text.to_string() == "Stack overflow");
    assert!(overflow, "{deeper:?}");
}

/// A value whose properties scripts read and write through its indexer: each name holds its
/// length until a script writes it, and `z` raises.
#[derive(Debug, Clone)]
struct Bag(Rc<RefCell<Vec<(String, INT)>>>);

impl Bag {
    fn get(&mut self, name: &str) -> Result<Dynamic, Box<EvalAltResult>> {
        if name == "z" {
            return Err(Raised::error(NumError::Overflow).into());
        }
        let written = self
            .0
            .borrow()
            .iter()
            .find(|(at, _)| at == name)
            .map(|&(_, v)| v);
        Ok(Dynamic::from(
            written.unwrap_or(INT::try_from(name.len()).unwrap()),
        ))
    }

    fn set(&mut self, name: &str, value: INT) {
        self.0.borrow_mut().push((name.to_owned(), value));
    }

    /// Binds the indexer, and forwards the properties too when `forward`.
    fn bind(host: &mut ScriptHost, forward: bool) {
        let engine = host.engine_mut();
        engine.register_type::<Bag>();
        engine.register_indexer_get(|bag: &mut Bag, name: ImmutableString| bag.get(&name));
        engine.register_indexer_set(|bag: &mut Bag, name: ImmutableString, value: INT| {
            bag.set(&name, value);
        });
        if forward {
            host.forward_properties(|engine, name| {
                let property = ImmutableString::from(name);
                let set = property.clone();
                engine.register_fn(format!("get${name}"), move |bag: &mut Bag| {
                    bag.get(&property)
                });
                engine.register_fn(format!("set${name}"), move |bag: &mut Bag, value: INT| {
                    bag.set(&set, value);
                });
            });
        }
    }
}

#[test]
fn a_forwarded_property_reads_and_writes_as_its_indexer_does_in_one_operation_less() {
    let source = "fn read(b) { b.abc + b.de } fn write(b) { b.abc = 7; b.abc } fn raise(b) { b.z }";
    // Forwards bound before the compile, and after it: each covers the names either way.
    let mut plain = host();
    Bag::bind(&mut plain, false);
    let plain_script = plain.compile(source).unwrap();
    let mut early = host();
    Bag::bind(&mut early, true);
    let early_script = early.compile(source).unwrap();
    let mut late = host();
    let late_script = late.compile(source).unwrap();
    Bag::bind(&mut late, true);

    let bag = || Bag(Rc::default());
    // `read` reads two properties, `write` writes one and reads one: the plain host spends a
    // second operation on each, the failed getter before its indexer.
    for (hook, value, accesses) in [("read", 3 + 2, 2), ("write", 7, 2)] {
        let mut plain_budget = ample();
        let read = plain.call(&mut plain_budget, plain_script, hook, (bag(),));
        assert_eq!(read.unwrap().as_int(), Ok(value), "{hook}");
        for (forwarded, script) in [(&mut early, early_script), (&mut late, late_script)] {
            let mut budget = ample();
            let read = forwarded.call(&mut budget, script, hook, (bag(),));
            assert_eq!(read.unwrap().as_int(), Ok(value), "{hook}");
            assert_eq!(budget.left() - plain_budget.left(), accesses, "{hook}");
        }
    }
    // The indexer's own error, raised the same either way.
    for (host, script) in [(&mut plain, plain_script), (&mut early, early_script)] {
        let raised = host.call(&mut ample(), script, "raise", (bag(),)).err();
        let overflow = matches!(&raised, Some(ScriptError::Raised(value))
            if value.get::<NumError>() == Some(NumError::Overflow));
        assert!(overflow, "{raised:?}");
    }
}
