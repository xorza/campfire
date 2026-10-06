use campfire_math::Num;
use rhai::{Engine, EvalAltResult, INT};

use crate::script_host::error::NumError;
use crate::script_host::error::Raised;

type Checked<T> = Result<T, Box<EvalAltResult>>;
type Operator = fn(Num, Num) -> Option<Num>;
type Comparison = fn(&Num, &Num) -> bool;
type Extreme = fn(Num, Num) -> Num;

/// `Num` in scripts: `num(i)`, the four operators, the comparisons, `min` and `max` with a `Num` or
/// an integer on either side, unary minus, `clamp`, and `round`, `floor` and `ceil` to an integer.
/// Arithmetic is checked: an overflow, or an integer beyond a `Num`, fails the call with its
/// `NumError`.
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
            ("*", Num::checked_mul),
            ("/", Num::checked_div),
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
        engine.register_fn("clamp", |n: Num, low: Num, high: Num| -> Checked<Num> {
            if low > high {
                return Err(Raised::error(NumError::ClampBounds).into());
            }
            Ok(n.clamp(low, high))
        });
        engine.register_fn("round", |n: Num| n.round());
        engine.register_fn("floor", |n: Num| n.floor());
        engine.register_fn("ceil", |n: Num| n.ceil());
    }
}

/// An integer as a `Num`; one beyond a `Num` fails the call.
fn from_int(value: INT) -> Checked<Num> {
    Num::from_int(value).ok_or_else(|| Raised::error(NumError::IntegerBeyondNum).into())
}

fn checked(value: Option<Num>) -> Checked<Num> {
    value.ok_or_else(|| Raised::error(NumError::Overflow).into())
}
