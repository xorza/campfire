use campfire_math::{Num, Rounding, Vec3};
use rhai::{Dynamic, Engine, EvalAltResult, INT};

use crate::script_host::error::{NumError, Raised};

type Checked<T> = Result<T, Box<EvalAltResult>>;
type Operator = fn(Num, Num) -> Option<Num>;
type Comparison = fn(&Num, &Num) -> bool;
type Extreme = fn(Num, Num) -> Num;

/// `Num` in scripts: `num(i)`, the four operators, the comparisons, `min` and `max` with a `Num` or
/// an integer on either side, unary minus, `clamp` with a `Num` or an integer in each place and a
/// `Num` in one at least, and `round`, `floor` and `ceil` to an integer;
/// and `vector(x, y, z)`, a vector of three `Num`s or integers. Arithmetic is checked: an
/// overflow, or an integer beyond a `Num`, fails the call with its `NumError`.
#[derive(Debug)]
pub(crate) struct NumApi;

impl NumApi {
    pub(crate) fn register(engine: &mut Engine) {
        engine.register_type_with_name::<Num>("Num");
        engine.register_fn("num", from_int);
        engine.register_fn("to_string", |n: Num| n.to_string());
        engine.register_fn("to_debug", |n: Num| n.to_string());

        let operators: [(&str, Operator); 4] = [
            ("+", Num::checked_add),
            ("-", Num::checked_sub),
            ("*", |a, b| a.checked_mul(b, Rounding::NearestEven)),
            ("/", |a, b| a.checked_div(b, Rounding::NearestEven)),
        ];
        for (name, operator) in operators {
            engine.register_fn(name, move |a: Num, b: Num| checked(operator(a, b)));
            engine.register_fn(name, move |a: Num, b: INT| {
                checked(operator(a, from_int(b)?))
            });
            engine.register_fn(name, move |a: INT, b: Num| {
                checked(operator(from_int(a)?, b))
            });
        }
        engine.register_fn("-", |n: Num| checked(n.checked_neg()));

        let comparisons: [(&str, Comparison); 6] = [
            ("==", Num::eq),
            ("!=", Num::ne),
            ("<", Num::lt),
            ("<=", Num::le),
            (">", Num::gt),
            (">=", Num::ge),
        ];
        for (name, compare) in comparisons {
            engine.register_fn(name, move |a: Num, b: Num| compare(&a, &b));
            engine.register_fn(name, move |a: Num, b: INT| {
                Ok::<_, Box<EvalAltResult>>(compare(&a, &from_int(b)?))
            });
            engine.register_fn(name, move |a: INT, b: Num| {
                Ok::<_, Box<EvalAltResult>>(compare(&from_int(a)?, &b))
            });
        }

        let extremes: [(&str, Extreme); 2] = [("min", Ord::min), ("max", Ord::max)];
        for (name, extreme) in extremes {
            engine.register_fn(name, move |a: Num, b: Num| extreme(a, b));
            engine.register_fn(name, move |a: Num, b: INT| -> Checked<Num> {
                Ok(extreme(a, from_int(b)?))
            });
            engine.register_fn(name, move |a: INT, b: Num| -> Checked<Num> {
                Ok(extreme(from_int(a)?, b))
            });
        }
        engine.register_fn("clamp", clamp::<Num, Num, Num>);
        engine.register_fn("clamp", clamp::<Num, Num, INT>);
        engine.register_fn("clamp", clamp::<Num, INT, Num>);
        engine.register_fn("clamp", clamp::<Num, INT, INT>);
        engine.register_fn("clamp", clamp::<INT, Num, Num>);
        engine.register_fn("clamp", clamp::<INT, Num, INT>);
        engine.register_fn("clamp", clamp::<INT, INT, Num>);
        engine.register_fn(
            "vector",
            |x: Dynamic, y: Dynamic, z: Dynamic| -> Checked<Vec3> {
                Ok(Vec3::new(coordinate(x)?, coordinate(y)?, coordinate(z)?))
            },
        );
        engine.register_fn("round", |n: Num| n.round());
        engine.register_fn("floor", |n: Num| n.floor());
        engine.register_fn("ceil", |n: Num| n.ceil());
    }
}

/// A number a function of `Num` takes in place of one: a `Num`, or an integer.
trait NumArg: Copy + 'static {
    fn num(self) -> Checked<Num>;
}

impl NumArg for Num {
    fn num(self) -> Checked<Num> {
        Ok(self)
    }
}

impl NumArg for INT {
    fn num(self) -> Checked<Num> {
        from_int(self)
    }
}

/// `n` within `low` and `high`; a low bound above the high one fails the call.
fn clamp<N: NumArg, L: NumArg, H: NumArg>(n: N, low: L, high: H) -> Checked<Num> {
    let [n, low, high] = [n.num()?, low.num()?, high.num()?];
    if low > high {
        return Err(Raised::error(NumError::ClampBounds).into());
    }
    Ok(n.clamp(low, high))
}

/// An integer as a `Num`; one beyond a `Num` fails the call.
fn from_int(value: INT) -> Checked<Num> {
    Num::from_int(value).ok_or_else(|| Raised::error(NumError::IntegerBeyondNum).into())
}

/// A vector's coordinate, a `Num` or an integer; any other value fails the call.
fn coordinate(value: Dynamic) -> Checked<Num> {
    match value.as_int() {
        Ok(int) => from_int(int),
        Err(_) => value
            .try_cast::<Num>()
            .ok_or_else(|| Raised::error(NumError::NotNumber).into()),
    }
}

fn checked(value: Option<Num>) -> Checked<Num> {
    value.ok_or_else(|| Raised::error(NumError::Overflow).into())
}
