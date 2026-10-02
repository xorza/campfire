use campfire_math::Num;
use rhai::INT;

use super::*;
use crate::error::NumError;

const PER_CALL: u64 = 1000;

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
    }
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
    for body in [r#"import "x" as x; 1"#, "sleep(1)", "timestamp()"] {
        assert!(run(&mut host, body).is_err(), "{body}");
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
    assert!(other.left() < 1500);

    // The call depth is the engine's 32 in every build: Rhai's default is 8 in a debug build
    // and 64 in a release one, so 20 levels passing and 40 failing show the engine's limit.
    let down = host
        .compile("fn down(n) { if n > 0 { down(n - 1) } else { 0 } }")
        .unwrap();
    assert_eq!(
        host.call(&mut ample(), down, "down", (20_i64,))
            .unwrap()
            .as_int(),
        Ok(0)
    );
    assert!(matches!(
        host.call(&mut ample(), down, "down", (40_i64,)),
        Err(ScriptError::Runtime(_))
    ));
}
