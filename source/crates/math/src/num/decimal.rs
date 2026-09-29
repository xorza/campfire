use std::str;

use crate::num::error::ParseNumError;
use crate::num::{Num, narrow};

/// Fractional digits read exactly. Every midpoint between neighbouring values, `(2j + 1) / 2²⁵`,
/// is a decimal with exactly 25 fractional digits, so later digits only break a tie.
const EXACT_DIGITS: u32 = 25;
const EXACT_SCALE: u128 = 10_u128.pow(EXACT_DIGITS);
/// Integer parts above this are out of range; the cap keeps the accumulator from overflowing.
const INT_CAP: u128 = 1 << 40;
/// Sign, 12 integer digits, point, 24 fractional digits.
const MAX_LEN: usize = 38;

/// Reads `[-]digits[.digits]`, rounded to nearest, ties to even.
pub(super) const fn parse(text: &[u8]) -> Result<Num, ParseNumError> {
    let negative = !text.is_empty() && text[0] == b'-';
    let mut at = if negative { 1 } else { 0 };

    let int_start = at;
    let mut int: u128 = 0;
    while at < text.len() && text[at].is_ascii_digit() {
        if int <= INT_CAP {
            int = int * 10 + (text[at] - b'0') as u128;
        }
        at += 1;
    }
    if at == int_start {
        return Err(ParseNumError::Malformed);
    }

    let mut frac: u128 = 0;
    let mut digits = 0;
    let mut beyond_nonzero = false;
    if at < text.len() {
        if text[at] != b'.' {
            return Err(ParseNumError::Malformed);
        }
        at += 1;
        let frac_start = at;
        while at < text.len() && text[at].is_ascii_digit() {
            let digit = (text[at] - b'0') as u128;
            if digits < EXACT_DIGITS {
                frac = frac * 10 + digit;
                digits += 1;
            } else if digit != 0 {
                beyond_nonzero = true;
            }
            at += 1;
        }
        if at == frac_start || at < text.len() {
            return Err(ParseNumError::Malformed);
        }
    }

    let scaled = (frac * 10_u128.pow(EXACT_DIGITS - digits)) << Num::FRAC_BITS;
    let mut frac_bits = scaled / EXACT_SCALE;
    let twice_rest = 2 * (scaled % EXACT_SCALE);
    if twice_rest > EXACT_SCALE
        || (twice_rest == EXACT_SCALE && (beyond_nonzero || frac_bits & 1 == 1))
    {
        frac_bits += 1;
    }

    let magnitude = ((int << Num::FRAC_BITS) + frac_bits).cast_signed();
    match narrow(if negative { -magnitude } else { magnitude }) {
        Some(value) => Ok(value),
        None => Err(ParseNumError::OutOfRange),
    }
}

/// The exact decimal text of a `Num`.
#[derive(Debug)]
pub(super) struct Decimal {
    bytes: [u8; MAX_LEN],
    len: usize,
}

impl Decimal {
    pub(super) const fn new(value: Num) -> Decimal {
        let mut decimal = Decimal {
            bytes: [0; MAX_LEN],
            len: 0,
        };
        let bits = value.to_bits();
        if bits < 0 {
            decimal.push(b'-');
        }
        let magnitude = bits.unsigned_abs();
        decimal.push_int(magnitude >> Num::FRAC_BITS);

        // frac / 2²⁴ = frac · 5²⁴ / 10²⁴: exactly 24 decimal digits, trailing zeros dropped.
        let mut frac =
            (magnitude & ((1 << Num::FRAC_BITS) - 1)) as u128 * 5_u128.pow(Num::FRAC_BITS);
        if frac != 0 {
            decimal.push(b'.');
            let mut scale = 10_u128.pow(Num::FRAC_BITS - 1);
            while frac != 0 {
                decimal.push(ascii_digit(frac / scale));
                frac %= scale;
                scale /= 10;
            }
        }
        decimal
    }

    pub(super) const fn as_str(&self) -> &str {
        match str::from_utf8(self.bytes.split_at(self.len).0) {
            Ok(text) => text,
            Err(_) => panic!("decimal text is ASCII"),
        }
    }

    const fn push(&mut self, byte: u8) {
        self.bytes[self.len] = byte;
        self.len += 1;
    }

    const fn push_int(&mut self, value: u64) {
        let mut scale = 1;
        while value / scale >= 10 {
            scale *= 10;
        }
        while scale != 0 {
            self.push(ascii_digit((value / scale % 10) as u128));
            scale /= 10;
        }
    }
}

const fn ascii_digit(value: u128) -> u8 {
    debug_assert!(value < 10);
    #[expect(clippy::cast_possible_truncation, reason = "a single decimal digit")]
    let digit = value as u8;
    b'0' + digit
}
