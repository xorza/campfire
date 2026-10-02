use super::*;

#[test]
fn products_and_sums_are_exact() {
    // Within u128 the high half is 0.
    assert_eq!(U256::product(6, 7), U256 { high: 0, low: 42 });
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
    assert_eq!(U256::product(5, 1).round_shr(1), Some(2));
    assert_eq!(U256::product(7, 1).round_shr(1), Some(4));
    let wide = U256::product(1 << 127, 4).checked_add(U256::product(2, 1));
    assert_eq!(wide.unwrap().round_shr(2), Some(1 << 127));
    assert_eq!(U256::product(1 << 100, 1 << 100).round_shr(64), None);
    assert_eq!(U256::product(u128::MAX, 1).round_shr(1), Some(1 << 127));

    // 7 ÷ 2 = 3.5 ties to 4, 5 ÷ 2 to 2, 10 ÷ 4 = 2.5 to 2, 11 ÷ 4 = 2.75 to 3; (2¹²⁸ + 6) ÷ 3
    // is 2¹²⁸ ÷ 3 rounded, 113427455640312821154458202477256070485.33 to ...485, plus 2;
    // 2¹²⁸ ÷ 1 passes u128.
    assert_eq!(U256::product(7, 1).round_div(2), Some(4));
    assert_eq!(U256::product(5, 1).round_div(2), Some(2));
    assert_eq!(U256::product(10, 1).round_div(4), Some(2));
    assert_eq!(U256::product(11, 1).round_div(4), Some(3));
    let past = U256::product(1 << 64, 1 << 64).checked_add(U256::product(6, 1));
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
