use super::*;

/// The sum of `products`, as a number.
fn sum(products: &[(i64, i128)]) -> Num {
    let mut sum = ProductSum::ZERO;
    for &(a, b) in products {
        sum.add(a, b);
    }
    sum.saturating_num()
}

#[test]
fn a_sum_rounds_once_ties_to_even_and_stops_at_the_range() {
    let one = 1_i128 << 24;
    // 3.0 × 1.0 − 2.0 × 1.0, raw 3 × 2²⁴ × 2²⁴ − 2 × 2²⁴ × 2²⁴, is 2⁴⁸, the bits 2²⁴ of
    // 1.0; no product is 0.
    assert_eq!(sum(&[(3 << 24, one), (-2 << 24, one)]), Num::ONE);
    assert_eq!(sum(&[]), Num::ZERO);
    // Each sum of raw products divides by 2²⁴ once: 2²³ is half a bit, a tie to the even 0;
    // 3 × 2²³ is 1.5 bits, to 2; −3 × 2²³ to −2; 2²³ + 1 is past the half, to 1.
    let half = 1_i128 << 23;
    let bits = |products: &[(i64, i128)]| sum(products).to_bits();
    assert_eq!(bits(&[(1, half)]), 0);
    assert_eq!(bits(&[(3, half)]), 2);
    assert_eq!(bits(&[(-3, half)]), -2);
    assert_eq!(bits(&[(1, half), (1, 1)]), 1);
    // The ends of the range: (2⁶³ − 1) × 2²⁴ is `MAX` exactly, and −2⁶³ × 2²⁴ `MIN`; one
    // bit more past each saturates, as does a product of 2¹⁹⁰ that alone passes u128.
    assert_eq!(sum(&[(i64::MAX, one)]), Num::MAX);
    assert_eq!(sum(&[(i64::MIN, one)]), Num::MIN);
    assert_eq!(sum(&[(i64::MAX, one), (1, one)]), Num::MAX);
    assert_eq!(sum(&[(i64::MIN, one), (-1, one)]), Num::MIN);
    assert_eq!(sum(&[(i64::MIN, i128::MIN)]), Num::MAX);
    assert_eq!(sum(&[(i64::MAX, i128::MIN)]), Num::MIN);
    // Products past the range that cancel stay exact: 2¹⁹⁰ from −2⁶³ × −2¹²⁷, then
    // −2¹⁹⁰ + 2⁶³ from −2⁶³ × (2¹²⁷ − 1), then −2⁶³, leave only 5 × 2⁴⁸, which is 5.0.
    let products = [
        (i64::MIN, i128::MIN),
        (i64::MIN, i128::MAX),
        (i64::MIN, 1),
        (5 << 24, one),
    ];
    assert_eq!(sum(&products), Num::int(5));
    // A rounding at an end: (2⁶³ − 1 + 0.5) bits ties to the even 2⁶³, past `MAX`;
    // −2⁶³ − 0.5 ties to the even −2⁶³, which is `MIN`; 2⁶³ − 1.5 ties to the even 2⁶³ − 2.
    assert_eq!(sum(&[(i64::MAX, one), (1, half)]), Num::MAX);
    assert_eq!(sum(&[(i64::MIN, one), (-1, half)]), Num::MIN);
    assert_eq!(bits(&[(i64::MAX, one), (-1, half)]), i64::MAX - 1);
}
