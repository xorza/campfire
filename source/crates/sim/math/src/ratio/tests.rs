use proptest::prelude::*;

use super::*;
use crate::u256::tests::limbs_times;

const CASES: u32 = 10_000;

/// `a / b` against `c / d`, `b` and `d` positive, by their continued fractions: the whole parts
/// first, then the reciprocals of the rests, which order the other way. Exact in `i128`, with no
/// product, so it shares no step with the cross products.
fn by_division(mut a: i128, mut b: i128, mut c: i128, mut d: i128) -> Ordering {
    loop {
        let (whole, other_whole) = (a.div_euclid(b), c.div_euclid(d));
        if whole != other_whole {
            return whole.cmp(&other_whole);
        }
        let (rest, other_rest) = (a.rem_euclid(b), c.rem_euclid(d));
        match (rest, other_rest) {
            (0, 0) => return Ordering::Equal,
            (0, _) => return Ordering::Less,
            (_, 0) => return Ordering::Greater,
            _ => (a, b, c, d) = (d, other_rest, b, rest),
        }
    }
}

/// `a / b` against `c / d` by their schoolbook cross products.
fn by_digits(a: U256, b: u128, c: U256, d: u128) -> Ordering {
    limbs_times(a, d)
        .iter()
        .rev()
        .cmp(limbs_times(c, b).iter().rev())
}

#[test]
fn shares_compare_by_value() {
    let f = Ratio::<i128, i128>::new;
    // 1/2 = 2/4; 1/3 < 1/2; −1/2 < −1/3 < 0 < 1/3.
    assert_eq!(f(1, 2), f(2, 4));
    assert!(f(1, 3) < f(1, 2));
    assert!(f(-1, 2) < f(-1, 3));
    assert!(f(-1, 3) < Ratio::<i128, i128>::ZERO && Ratio::<i128, i128>::ZERO < f(1, 3));
    assert_eq!(f(0, 7), Ratio::<i128, i128>::ZERO);
    assert_eq!(f(5, 5), Ratio::<i128, i128>::ONE);
    // Past u128 in the products: (2¹²⁶ + 1) / 2¹²⁶ just above 1, and (2¹²⁶ − 1) / 2¹²⁶ just
    // below; negated, the other way. At the ends, (2¹²⁷ − 1) / 1 passes (2¹²⁷ − 2) / 1 over one
    // denominator, and i128::MIN / (2¹²⁷ − 1) lies just below −1.
    let big = 1_i128 << 126;
    assert!(f(big + 1, big) > Ratio::<i128, i128>::ONE);
    assert!(f(big - 1, big) < Ratio::<i128, i128>::ONE);
    assert!(f(-(big + 1), big) < f(-(big - 1), big));
    assert!(f(i128::MAX, 1) > f(i128::MAX - 1, 1));
    assert!(f(i128::MIN, i128::MAX) < f(-1, 1) && f(-1, 1) < f(i128::MIN + 2, i128::MAX));
    // A share of a number: 2/5 of 10 m is 4 m; 1/3 of a bit rounds to none, 2/3 to one.
    assert_eq!(f(2, 5).of(Num::int(10)), Num::int(4));
    assert_eq!(f(1, 3).of(Num::EPSILON), Num::ZERO);
    assert_eq!(f(2, 3).of(Num::EPSILON), Num::EPSILON);
    assert_eq!(Ratio::<i128, i128>::ONE.of(Num::int(7)), Num::int(7));
}

#[test]
fn squared_distances_compare_by_value() {
    let d = |num: u128, den: u128| Ratio::<U256, u128>::new(U256::from_u128(num), den);
    let whole = |num: u128| Ratio::whole(U256::from_u128(num));
    // 9/4 = 18/8; 2 < 9/4 < 3; a whole square is over 1.
    assert_eq!(d(9, 4), d(18, 8));
    assert!(whole(2) < d(9, 4) && d(9, 4) < whole(3));
    assert_eq!(Ratio::<U256, u128>::ZERO, d(0, 5));
    // At the bound: 2¹⁸⁴ over 2⁹² equals 2⁹² over 1, and one more in the numerator is past it;
    // two over the same large denominator compare by numerator alone.
    let wide = Ratio::new(U256::product(1 << 92, 1 << 92), 1 << 92);
    assert_eq!(wide, whole(1 << 92));
    let above = U256::product(1 << 92, 1 << 92).checked_add(U256::from_u128(1));
    assert!(Ratio::new(above.unwrap(), 1 << 92) > wide);
    // A path's distance over 2⁹² against an edge's over 2⁷³, whose cross products pass 256 bits:
    // (2¹⁸⁴ + 2⁹¹) / 2⁹² and (2¹⁶⁵ + 2⁷²) / 2⁷³ are both 2⁹² + ½; one more in the first numerator
    // is past the second, both ways.
    let over = |high: u32, half: u32, extra: u128, den: u32| {
        let num = U256::product(1 << high, 1 << high)
            .checked_add(U256::from_u128(1 << half))
            .and_then(|num| num.checked_add(U256::from_u128(extra)));
        Ratio::new(num.unwrap(), 1 << den)
    };
    let edge_num = |extra: u32| {
        U256::product(1 << 82, 1 << 83)
            .checked_add(U256::from_u128(1 << extra))
            .unwrap()
    };
    let path = over(92, 91, 0, 92);
    let edge = Ratio::new(edge_num(72), 1 << 73);
    assert_eq!(path, edge);
    assert_eq!(edge, path);
    let past = over(92, 91, 1, 92);
    assert_eq!(past.cmp(&edge), Ordering::Greater);
    assert_eq!(edge.cmp(&past), Ordering::Less);
    // A whole bit apart, past 256 bits too: 2¹⁸⁴ / 2⁹² against (2¹⁶⁵ + 2⁷³) / 2⁷³, 2⁹² + 1.
    let next = Ratio::new(edge_num(73), 1 << 73);
    assert_eq!(wide.cmp(&next), Ordering::Less);
    assert_eq!(next.cmp(&wide), Ordering::Greater);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(CASES))]

    /// Two shares of any numerators and positive denominators.
    #[test]
    fn shares_order_as_their_continued_fractions_do(
        a in any::<i128>(),
        b in 1_i128..,
        c in any::<i128>(),
        d in 1_i128..,
    ) {
        let (first, second) = (Ratio::new(a, b), Ratio::new(c, d));
        let expected = by_division(a, b, c, d);
        prop_assert_eq!(first.cmp(&second), expected);
        prop_assert_eq!(second.cmp(&first), expected.reverse());
    }

    /// A share against itself scaled by `k` is equal, and against one bit more or less in its
    /// numerator is not: the cases a draw of four values almost never meets.
    #[test]
    fn a_share_equals_its_multiple_and_not_its_neighbours(
        a in -(1_i128 << 100)..(1_i128 << 100),
        b in 1_i128..(1_i128 << 100),
        k in 1_i128..(1_i128 << 26),
    ) {
        let share = Ratio::new(a, b);
        prop_assert_eq!(share, Ratio::new(a * k, b * k));
        prop_assert!(Ratio::new(a * k - 1, b * k) < share);
        prop_assert!(Ratio::new(a * k + 1, b * k) > share);
    }

    /// Two squared distances of any numerators and positive denominators.
    #[test]
    fn squared_distances_order_as_their_schoolbook_products_do(
        a in (any::<u128>(), any::<u128>()),
        b in 1_u128..,
        c in (any::<u128>(), any::<u128>()),
        d in 1_u128..,
        narrow in 0_u32..4,
    ) {
        // Numerators of every width: both past 128 bits, one, or neither.
        let fit = |(high, low): (u128, u128), keep: bool| U256::from_halves(if keep { high } else { 0 }, low);
        let (a, c) = (fit(a, narrow & 1 == 0), fit(c, narrow & 2 == 0));
        let order = Ratio::new(a, b).cmp(&Ratio::new(c, d));
        prop_assert_eq!(order, by_digits(a, b, c, d));
    }

    /// A squared distance against itself scaled by `k` is equal, and against one more in its
    /// numerator is less.
    #[test]
    fn a_squared_distance_equals_its_multiple_and_not_its_neighbour(
        a in (0_u128..(1 << 100), any::<u128>()),
        b in 1_u128..(1 << 100),
        k in 1_u128..(1 << 27),
    ) {
        let num = U256::from_halves(a.0, a.1);
        let distance = Ratio::new(num, b);
        let scaled = num.checked_mul(k).unwrap();
        prop_assert_eq!(distance, Ratio::new(scaled, b * k));
        let above = scaled.checked_add(U256::from_u128(1)).unwrap();
        prop_assert!(Ratio::new(above, b * k) > distance);
    }
}
