use proptest::prelude::*;

use crate::num::Num;
use crate::num::error::ParseNumError;
use crate::num::tests::{CASES, HALF, ONE, QUARTER, bits, exact, n, nearest_even};

#[test]
fn parse_reads_exact_decimals() {
    // 2⁻²⁴ = 0.000000059604644775390625, 2⁻²⁵ = 0.0000000298023223876953125 (25 digits).
    let cases = [
        ("0", 0),
        ("-0", 0),
        ("1", ONE),
        ("7.5", 7 * ONE + HALF),
        ("-0.25", -QUARTER),
        ("0.000000059604644775390625", 1),
        // Exactly half of ε ties to the even 0; any later non-zero digit moves it above the tie.
        ("0.0000000298023223876953125", 0),
        ("0.00000002980232238769531250000000001", 1),
        ("0.0000000298023223876953124999999999", 0),
        // 1.5ε ties to the even 2ε; −0.5ε ties to 0.
        ("0.0000000894069671630859375", 2),
        ("-0.0000000298023223876953125", 0),
        // 2³⁹ − 2⁻²⁴ and −2³⁹ are MAX and MIN.
        ("549755813887.999999940395355224609375", i64::MAX),
        // The midpoint between MAX and 2³⁹ is 2³⁹ − 2.98…·10⁻⁸; 3·10⁻⁸ below 2³⁹ is under it.
        ("549755813887.99999997", i64::MAX),
        ("-549755813888", i64::MIN),
        // Less than half an ε below −2³⁹ still rounds to MIN.
        ("-549755813888.00000000000000000000000001", i64::MIN),
        ("000123.500", 123 * ONE + HALF),
    ];
    for (text, bits) in cases {
        assert_eq!(text.parse::<Num>(), Ok(n(bits)), "{text}");
    }
}

#[test]
fn parse_rejects_bad_text() {
    for text in [
        "", "-", "+1", "1.", ".5", "1.2.3", " 1", "1 ", "1e3", "1_000", "--1", "0x10",
    ] {
        assert_eq!(
            text.parse::<Num>(),
            Err(ParseNumError::Malformed),
            "{text:?}"
        );
    }
    for text in [
        "549755813888",
        "549755813887.99999998",
        "-549755813888.00000003",
        "99999999999999999999999999999999999999999",
    ] {
        assert_eq!(
            text.parse::<Num>(),
            Err(ParseNumError::OutOfRange),
            "{text}"
        );
    }
}

#[test]
fn display_is_exact() {
    let cases = [
        (0, "0"),
        (ONE, "1"),
        (7 * ONE + HALF, "7.5"),
        (-QUARTER, "-0.25"),
        (1, "0.000000059604644775390625"),
        (-1, "-0.000000059604644775390625"),
        (i64::MAX, "549755813887.999999940395355224609375"),
        (i64::MIN, "-549755813888"),
    ];
    for (bits, text) in cases {
        assert_eq!(n(bits).to_string(), text);
    }
    assert_eq!(
        format!("{:>6}|{:<6}|", Num::ONE, Num::ONE),
        "     1|1     |"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    #[test]
    fn display_parses_back(a in bits()) {
        prop_assert_eq!(n(a).to_string().parse::<Num>(), Ok(n(a)));
    }

    #[test]
    fn parse_matches_exact_rounding(
        negative in any::<bool>(),
        int in 0_u64..1_000_000_000_000,
        frac in "[0-9]{0,18}",
    ) {
        let text = format!("{}{int}{}{frac}", if negative { "-" } else { "" }, if frac.is_empty() { "" } else { "." });
        let scale = 10_i128.pow(u32::try_from(frac.len()).unwrap());
        let digits = i128::from(int) * scale + frac.parse::<i128>().unwrap_or(0);
        let magnitude = nearest_even(digits << 24, scale);
        let expected = exact(if negative { -magnitude } else { magnitude });
        prop_assert_eq!(text.parse::<Num>().ok(), expected);
    }
}
