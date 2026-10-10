use super::*;
use crate::rng::split_mix64::SplitMix64;

#[test]
fn products_and_sums_are_exact() {
    // Within u128 the high half is 0, as a widened value's is.
    assert_eq!(U256::product(6, 7), U256 { high: 0, low: 42 });
    assert_eq!(U256::from_u128(42), U256::product(6, 7));
    assert_eq!(U256::from_u128(u128::MAX).high, 0);
    // 2¹²⁷ × 4 = 2¹²⁹: bit 1 of the high half.
    assert_eq!(U256::product(1 << 127, 4), U256 { high: 2, low: 0 });
    // (2¹²⁸ − 1)² = 2²⁵⁶ − 2¹²⁹ + 1: high 2¹²⁸ − 2, low 1.
    let max = U256::product(u128::MAX, u128::MAX);
    assert_eq!(
        max,
        U256 {
            high: u128::MAX - 1,
            low: 1
        }
    );
    // (2⁶⁴ + 3)(2⁶⁴ + 5) = 2¹²⁸ + 8 × 2⁶⁴ + 15: every cross term carries into place.
    let cross = U256::product((1 << 64) + 3, (1 << 64) + 5);
    assert_eq!(
        cross,
        U256 {
            high: 1,
            low: (8 << 64) + 15
        }
    );

    // A carry from the low half; and past 256 bits, none.
    let one_short = U256 {
        high: 0,
        low: u128::MAX,
    };
    let one = U256 { high: 0, low: 1 };
    assert_eq!(one_short.checked_add(one), Some(U256 { high: 1, low: 0 }));
    let top = U256 {
        high: u128::MAX,
        low: u128::MAX,
    };
    assert_eq!(top.checked_add(one), None);
    // A borrow from the high half; a difference of 0; and below 0, none.
    let carried = U256 { high: 1, low: 0 };
    assert_eq!(carried.checked_sub(one), Some(one_short));
    assert_eq!(top.checked_sub(top), Some(U256::ZERO));
    assert_eq!(one_short.checked_sub(carried), None);
    assert_eq!(U256::ZERO.checked_sub(one), None);

    // A product with a high half: (2¹²⁸ + 2¹²⁸ − 1) × 2 = 2¹³⁰ − 2, so high 3 and low
    // 2¹²⁸ − 2; 2²⁵⁵ × 2 and the high half's own carry pass 256 bits.
    let both = U256 {
        high: 1,
        low: u128::MAX,
    };
    assert_eq!(
        both.checked_mul(2),
        Some(U256 {
            high: 3,
            low: u128::MAX - 1
        })
    );
    let half_top = U256 {
        high: 1 << 127,
        low: 0,
    };
    assert_eq!(half_top.checked_mul(2), None);
    assert_eq!(top.checked_mul(1), Some(top));
    assert_eq!(top.checked_mul(0), Some(U256::ZERO));
    // (2¹²⁸ − 1) / 3 in the high half times 3 fills it, and the low half's product carries 2
    // into it, past 256 bits: the high half fits alone and the sum does not.
    let into_high = U256 {
        high: u128::MAX / 3,
        low: u128::MAX,
    };
    assert_eq!(into_high.checked_mul(3), None);
    assert_eq!(top.checked_mul(2), None);

    // The order compares the high half first.
    assert!(
        U256 { high: 1, low: 0 }
            > U256 {
                high: 0,
                low: u128::MAX
            }
    );
    assert!(U256::product(3, 5) < U256::product(4, 4));

    // Halving 5 gives 2.5, ties to the even 2; 7 gives 3.5, to 4; 2¹²⁹ + 2¹ shifted by 2
    // is 2¹²⁷ + 0.5, to the even 2¹²⁷; 2²⁰⁰ shifted by 64 passes u128.
    assert_eq!(U256::from_u128(5).round_shr(1), Some(2));
    assert_eq!(U256::from_u128(7).round_shr(1), Some(4));
    let wide = U256::product(1 << 127, 4).checked_add(U256::from_u128(2));
    assert_eq!(wide.unwrap().round_shr(2), Some(1 << 127));
    assert_eq!(U256::product(1 << 100, 1 << 100).round_shr(64), None);
    assert_eq!(U256::from_u128(u128::MAX).round_shr(1), Some(1 << 127));

    // 7 ÷ 2 = 3.5 ties to 4, 5 ÷ 2 to 2, 10 ÷ 4 = 2.5 to 2, 11 ÷ 4 = 2.75 to 3; (2¹²⁸ + 6) ÷ 3
    // is 2¹²⁸ ÷ 3 rounded, 113427455640312821154458202477256070485.33 to ...485, plus 2;
    // 2¹²⁸ ÷ 1 passes u128.
    assert_eq!(U256::from_u128(7).round_div(2), Some(4));
    assert_eq!(U256::from_u128(5).round_div(2), Some(2));
    assert_eq!(U256::from_u128(10).round_div(4), Some(2));
    assert_eq!(U256::from_u128(11).round_div(4), Some(3));
    let past = U256::product(1 << 64, 1 << 64).checked_add(U256::from_u128(6));
    assert_eq!(
        past.unwrap().round_div(3),
        Some(113_427_455_640_312_821_154_458_202_477_256_070_485 + 2)
    );
    assert_eq!(U256::product(1 << 64, 1 << 64).round_div(1), None);

    // 2¹²⁹ − 1 halved is 2¹²⁸ − 0.5: its floor is u128::MAX, odd, so the tie rounds up, past
    // u128, by the shift and by the division alike.
    let odd = U256 {
        high: 1,
        low: u128::MAX,
    };
    assert_eq!((odd.round_shr(1), odd.round_div(2)), (None, None));
    // The widest shift and the widest divisor: 2²⁵⁴ shifted by 127 is 2¹²⁷; (2¹²⁷ − 1)² over
    // 2¹²⁷ − 1 is 2¹²⁷ − 1.
    assert_eq!(
        U256::product(1 << 127, 1 << 127).round_shr(127),
        Some(1 << 127)
    );
    let widest = (1 << 127) - 1;
    assert_eq!(
        U256::product(widest, widest).round_div(widest),
        Some(widest)
    );
}

/// `value / divisor` and its rest a bit at a time, for a high half below the divisor: the rest
/// below the divisor, doubled, plus a bit, stays below 2¹²⁹, so its top bit is carried apart.
fn by_bits(value: U256, divisor: u128) -> Division {
    let (mut rest, mut quotient) = (value.high, 0_u128);
    for bit in (0..128).rev() {
        let carry = rest >> 127;
        rest = (rest << 1) | ((value.low >> bit) & 1);
        quotient <<= 1;
        if carry == 1 || rest >= divisor {
            rest = rest.wrapping_sub(divisor);
            quotient |= 1;
        }
    }
    Division { quotient, rest }
}

#[test]
fn a_long_division_is_the_division_a_bit_at_a_time() {
    // Drawn divisors and dividends of every length, so the shift that sets the divisor's top bit
    // and each digit's corrections run at every size; the high half below the divisor.
    let mut words = SplitMix64::new(0x5EED);
    let mut wide = || {
        let value = (u128::from(words.next_u64()) << 64) | u128::from(words.next_u64());
        value >> (words.next_u64() % 128)
    };
    for _ in 0..20_000 {
        let divisor = wide().max(1);
        let value = U256 {
            high: wide() % divisor,
            low: wide(),
        };
        let case = format!("{value:?} {divisor}");
        assert_eq!(
            value.long_division(divisor),
            by_bits(value, divisor),
            "{case}"
        );
    }
}

/// `value × by` by schoolbook multiplication of 32-bit limbs, the least significant first.
fn limbs_times(value: U256, by: u128) -> [u64; 12] {
    let limbs = |half: u128| [0, 1, 2, 3].map(|at| (half >> (32 * at)) & u128::from(u32::MAX));
    let value: Vec<u128> = limbs(value.low)
        .into_iter()
        .chain(limbs(value.high))
        .collect();
    let by = limbs(by);
    let mut out = [0_u64; 12];
    for (i, &x) in value.iter().enumerate() {
        let mut carry = 0_u128;
        for (j, &y) in by.iter().enumerate() {
            let sum = x * y + u128::from(out[i + j]) + carry;
            out[i + j] = u64::try_from(sum & u128::from(u32::MAX)).unwrap();
            carry = sum >> 32;
        }
        out[i + 4] = u64::try_from(carry).unwrap();
    }
    out
}

/// The product of `a` and `b` by schoolbook multiplication of 32-bit limbs.
fn schoolbook(a: u128, b: u128) -> U256 {
    let limbs = |value: u128| {
        [0, 1, 2, 3].map(|at| u32::try_from((value >> (32 * at)) & u128::from(u32::MAX)).unwrap())
    };
    let (a, b) = (limbs(a), limbs(b));
    let mut out = [0_u64; 8];
    for (i, &x) in a.iter().enumerate() {
        let mut carry = 0_u64;
        for (j, &y) in b.iter().enumerate() {
            let sum = u64::from(x) * u64::from(y) + (out[i + j] & 0xFFFF_FFFF) + carry;
            out[i + j] = sum & 0xFFFF_FFFF;
            carry = sum >> 32;
        }
        out[i + 4] = carry;
    }
    let join = |half: &[u64]| {
        half.iter()
            .rev()
            .fold(0_u128, |acc, &limb| acc << 32 | u128::from(limb))
    };
    U256 {
        high: join(&out[4..]),
        low: join(&out[..4]),
    }
}

#[test]
fn products_match_schoolbook_and_shifts_match_divisions() {
    let values = [
        0,
        1,
        3,
        (1 << 32) - 1,
        1 << 32,
        (1 << 64) + 3,
        0xDEAD_BEEF_0123_4567_89AB_CDEF_F00D_CAFE,
        (1 << 127) - 1,
        1 << 127,
        u128::MAX - 1,
        u128::MAX,
    ];
    for &a in &values {
        for &b in &values {
            let product = U256::product(a, b);
            assert_eq!(product, schoolbook(a, b), "{a} {b}");
            // A low half alone times a u128 is the product of the two.
            assert_eq!(U256::from_u128(a).checked_mul(b), Some(product), "{a} {b}");
            // Rounded up, a product's quotient is the one a bit at a time, or one more with a
            // rest, while it fits u128, as it does exactly when the high half is below the
            // divisor.
            for &divisor in values.iter().filter(|&&d| d > 0) {
                let up = (product.high < divisor)
                    .then(|| by_bits(product, divisor))
                    .and_then(|Division { quotient, rest }| {
                        quotient.checked_add(u128::from(rest != 0))
                    });
                assert_eq!(product.div_ceil(divisor), up, "{a} {b} {divisor}");
            }
            // A product times a third value orders against another as their limbs do.
            for &by in &values {
                let ours = limbs_times(product, by);
                for &(c, d) in &[(a, by), (b, a), (by, by)] {
                    let other = U256::product(c, d);
                    let theirs = limbs_times(other, b);
                    let order = product.cmp_products(by, other, b);
                    assert_eq!(
                        order,
                        ours.iter().rev().cmp(theirs.iter().rev()),
                        "{a} {b} {by}"
                    );
                }
            }
            // The long division of the product's low half, led by its high half's rest, is the
            // division a bit at a time.
            for &divisor in values.iter().filter(|&&d| d > 0) {
                let led = U256 {
                    high: product.high % divisor,
                    low: product.low,
                };
                assert_eq!(
                    led.long_division(divisor),
                    by_bits(led, divisor),
                    "{a} {b} {divisor}"
                );
            }
            // A shift by k is a division by 2^k, rounded the same way.
            for k in 1..=126 {
                assert_eq!(
                    product.round_shr(k),
                    product.round_div(1 << k),
                    "{a} {b} {k}"
                );
            }
        }
    }
}

#[test]
fn a_division_and_a_product_order_by_hand_at_the_ends() {
    // 2²⁵⁵ / 3: 2 to an odd power is 2 past a multiple of 3, so (2²⁵⁵ − 2) / 3 rest 2; its
    // quotient passes u128, so rounded up it is none. 2²⁵⁵ / (2¹²⁸ − 1) is 2¹²⁷ + 2¹²⁷ / (2¹²⁸ − 1),
    // 2¹²⁷ with a rest, so 2¹²⁷ + 1 up.
    let top = U256 {
        high: 1 << 127,
        low: 0,
    };
    assert_eq!(top.div_ceil(3), None);
    assert_eq!(top.div_ceil(u128::MAX), Some((1 << 127) + 1));
    let led = U256 {
        high: (1 << 127) % 3,
        low: 0,
    };
    assert_eq!(led.long_division(3).rest, 2);
    // 2¹²⁸ / 2 is 2¹²⁷ either way; (2¹²⁸ + 1) / 2 is 2¹²⁷ rest 1, so 2¹²⁷ + 1 up; 2²⁰⁰ / 2¹⁰⁰
    // is 2¹⁰⁰; 2²⁰⁰ / 2¹⁰ passes u128.
    let two_128 = U256::product(1 << 64, 1 << 64);
    assert_eq!(two_128.div_ceil(2), Some(1 << 127));
    let odd = two_128.checked_add(U256::from_u128(1)).unwrap();
    assert_eq!(odd.div_ceil(2), Some((1 << 127) + 1));
    let two_200 = U256::product(1 << 100, 1 << 100);
    assert_eq!(two_200.div_ceil(1 << 100), Some(1 << 100));
    assert_eq!(two_200.div_ceil(1 << 10), None);
    // 1 / 1 is 1; 1 / 2 is 0 rest 1, so 1 up.
    let one = U256::from_u128(1);
    assert_eq!((one.div_ceil(1), one.div_ceil(2)), (Some(1), Some(1)));
    // At 384 bits: (2²⁵⁶ − 1)(2¹²⁸ − 1) equals itself, and passes (2²⁵⁶ − 1)(2¹²⁸ − 2) by
    // 2²⁵⁶ − 1; 2²⁵⁵ · 1 is 2¹²⁸ · 2¹²⁷, and below 2¹²⁸ · (2¹²⁷ + 1); and 0 · 0 is 0 · 1.
    let full = U256 {
        high: u128::MAX,
        low: u128::MAX,
    };
    assert_eq!(
        full.cmp_products(u128::MAX, full, u128::MAX),
        Ordering::Equal
    );
    assert_eq!(
        full.cmp_products(u128::MAX, full, u128::MAX - 1),
        Ordering::Greater
    );
    assert_eq!(
        full.cmp_products(u128::MAX - 1, full, u128::MAX),
        Ordering::Less
    );
    assert_eq!(top.cmp_products(1, two_128, 1 << 127), Ordering::Equal);
    assert_eq!(top.cmp_products(1, two_128, (1 << 127) + 1), Ordering::Less);
    assert_eq!(U256::ZERO.cmp_products(0, U256::ZERO, 1), Ordering::Equal);
}
