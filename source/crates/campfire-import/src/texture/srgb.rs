/// The sRGB transfer function on 8-bit codes, in integers: a code's light, linear, in steps of
/// 2⁻¹⁶, so code 255 is 65536, and a mean of light back to the nearest code. sRGB's curve, `((v + 0.055) / 1.055)^2.4`
/// above code 10 and `v / 12.92` up to it, reduces on code `k` to `((40k + 561) / 10761)^(12/5)`,
/// which the tests check each entry of exactly in 256-bit integers.
#[derive(Debug)]
pub(crate) struct Srgb;

/// Each code's light, rounded to nearest.
const LIGHT: [u32; 256] = [
    0, 20, 40, 60, 80, 99, 119, 139, 159, 179, 199, 219, 241, 264, 288, 313, 340, 367, 396, 427,
    458, 491, 526, 562, 599, 637, 677, 718, 761, 805, 851, 898, 947, 997, 1048, 1101, 1156, 1212,
    1270, 1330, 1391, 1453, 1517, 1583, 1651, 1720, 1791, 1863, 1937, 2013, 2090, 2170, 2250, 2333,
    2418, 2504, 2592, 2681, 2773, 2866, 2961, 3058, 3157, 3258, 3360, 3464, 3570, 3678, 3788, 3900,
    4014, 4129, 4247, 4366, 4488, 4611, 4736, 4864, 4993, 5124, 5257, 5392, 5530, 5669, 5810, 5953,
    6099, 6246, 6395, 6547, 6701, 6856, 7014, 7174, 7336, 7500, 7666, 7834, 8004, 8177, 8352, 8529,
    8708, 8889, 9072, 9258, 9446, 9636, 9828, 10022, 10219, 10418, 10619, 10822, 11028, 11236,
    11446, 11658, 11873, 12090, 12309, 12531, 12754, 12981, 13209, 13440, 13673, 13909, 14147,
    14387, 14629, 14874, 15122, 15372, 15624, 15878, 16135, 16394, 16656, 16920, 17187, 17456,
    17727, 18001, 18278, 18556, 18838, 19121, 19408, 19696, 19988, 20281, 20578, 20876, 21178,
    21481, 21788, 22096, 22408, 22722, 23038, 23357, 23679, 24003, 24329, 24659, 24991, 25325,
    25662, 26002, 26344, 26689, 27036, 27387, 27739, 28095, 28453, 28813, 29177, 29543, 29911,
    30283, 30657, 31033, 31413, 31795, 32180, 32567, 32957, 33350, 33746, 34144, 34545, 34949,
    35355, 35765, 36177, 36591, 37009, 37429, 37852, 38278, 38707, 39138, 39572, 40009, 40449,
    40892, 41337, 41786, 42237, 42691, 43147, 43607, 44069, 44534, 45003, 45474, 45947, 46424,
    46904, 47386, 47871, 48360, 48851, 49345, 49842, 50342, 50844, 51350, 51859, 52370, 52884,
    53402, 53922, 54445, 54972, 55501, 56033, 56568, 57106, 57647, 58191, 58738, 59288, 59841,
    60397, 60956, 61518, 62083, 62651, 63222, 63796, 64373, 64953, 65536,
];

/// The light halfway between each code and the next, `(k + 0.5) / 255` on the curve, rounded to
/// nearest: a light at or past it encodes as the next code.
const MIDPOINTS: [u32; 255] = [
    10, 30, 50, 70, 90, 109, 129, 149, 169, 189, 209, 230, 252, 276, 300, 326, 353, 382, 411, 442,
    475, 508, 543, 580, 618, 657, 697, 739, 783, 828, 874, 922, 971, 1022, 1075, 1129, 1184, 1241,
    1300, 1360, 1422, 1485, 1550, 1617, 1685, 1755, 1827, 1900, 1975, 2051, 2130, 2210, 2292, 2375,
    2460, 2548, 2636, 2727, 2819, 2914, 3010, 3107, 3207, 3309, 3412, 3517, 3624, 3733, 3844, 3957,
    4071, 4188, 4306, 4427, 4549, 4674, 4800, 4928, 5058, 5190, 5325, 5461, 5599, 5739, 5882, 6026,
    6172, 6321, 6471, 6623, 6778, 6935, 7094, 7254, 7417, 7582, 7750, 7919, 8090, 8264, 8440, 8618,
    8798, 8980, 9165, 9351, 9540, 9731, 9925, 10120, 10318, 10518, 10720, 10925, 11131, 11340,
    11552, 11765, 11981, 12199, 12420, 12642, 12867, 13095, 13324, 13556, 13791, 14027, 14266,
    14508, 14752, 14998, 15246, 15497, 15751, 16006, 16264, 16525, 16788, 17053, 17321, 17591,
    17864, 18139, 18417, 18697, 18979, 19264, 19552, 19842, 20134, 20429, 20727, 21027, 21329,
    21634, 21942, 22252, 22564, 22879, 23197, 23517, 23840, 24166, 24494, 24824, 25157, 25493,
    25832, 26173, 26516, 26862, 27211, 27563, 27917, 28273, 28633, 28995, 29359, 29727, 30097,
    30469, 30845, 31223, 31603, 31987, 32373, 32762, 33153, 33548, 33944, 34344, 34747, 35152,
    35560, 35970, 36384, 36800, 37219, 37640, 38065, 38492, 38922, 39355, 39791, 40229, 40670,
    41114, 41561, 42011, 42463, 42919, 43377, 43838, 44302, 44768, 45238, 45710, 46185, 46664,
    47145, 47628, 48115, 48605, 49097, 49593, 50091, 50593, 51097, 51604, 52114, 52627, 53143,
    53662, 54183, 54708, 55236, 55766, 56300, 56837, 57376, 57919, 58464, 59013, 59564, 60118,
    60676, 61236, 61800, 62366, 62936, 63508, 64084, 64662, 65244,
];

impl Srgb {
    /// The light of `code`.
    pub(crate) const fn light(code: u8) -> u32 {
        LIGHT[code as usize]
    }

    /// The code nearest the mean light `sum / total`, `total` positive: the count of midpoints
    /// the mean reaches, compared exactly as `sum ≥ total · midpoint`.
    pub(crate) fn encode(sum: u64, total: u64) -> u8 {
        debug_assert!(total > 0, "a mean of something");
        let reached = MIDPOINTS.partition_point(|&midpoint| sum >= total * u64::from(midpoint));
        u8::try_from(reached).expect("at most 255 midpoints")
    }
}

#[cfg(test)]
mod tests {
    use campfire_math::U256;

    use super::*;

    /// The scale of light: 1 is `2¹⁶`.
    const SCALE: u32 = 1 << 16;

    /// The denominator of the curve's base, `269025 / 25`.
    const BASE: u128 = 10_761;

    /// Whether `light` is `SCALE · (n / BASE)^(12/5)` rounded to nearest: `(2L − 1)⁵ · BASE¹² ≤
    /// (2 · SCALE)⁵ · n¹² < (2L + 1)⁵ · BASE¹²`, each side under 2²⁴⁷.
    fn on_curve(light: u32, n: u128) -> bool {
        let side = |odd: u128| U256::product(odd.pow(5), BASE.pow(6)).checked_mul(BASE.pow(6));
        let target = U256::product((2 * u128::from(SCALE)).pow(5), n.pow(6)).checked_mul(n.pow(6));
        let (low, high) = (
            side(2 * u128::from(light) - 1),
            side(2 * u128::from(light) + 1),
        );
        low <= target && target < high
    }

    /// Whether `light` is `SCALE · num / den` rounded to nearest; `den` is never twice a tie.
    fn on_line(light: u32, num: u128, den: u128) -> bool {
        let target = 2 * u128::from(SCALE) * num;
        (2 * u128::from(light) - 1) * den <= target && target < (2 * u128::from(light) + 1) * den
    }

    #[test]
    fn each_entry_is_the_curve_rounded_to_nearest() {
        assert_eq!((LIGHT[0], LIGHT[255]), (0, SCALE));
        // Codes up to 10 lie on the line, 10/255 ≤ 0.04045; 12.92 · 255 = 3294.6.
        for (code, &light) in (0_u128..).zip(&LIGHT).take(255).skip(1) {
            let rounded = if code <= 10 {
                on_line(light, 100 * code, 329_460)
            } else {
                on_curve(light, 40 * code + 561)
            };
            assert!(rounded, "code {code}");
        }
        // Midpoints up to 9.5/255 lie on the line, the rest on the curve, at 40 (k + ½) + 561.
        for (k, &midpoint) in (0_u128..).zip(&MIDPOINTS) {
            let rounded = if k <= 9 {
                on_line(midpoint, 100 * (2 * k + 1), 658_920)
            } else {
                on_curve(midpoint, 40 * k + 581)
            };
            assert!(rounded, "midpoint {k}");
        }
        assert!((0..255).all(|k| LIGHT[k] < MIDPOINTS[k] && MIDPOINTS[k] < LIGHT[k + 1]));
    }

    #[test]
    fn a_mean_of_light_encodes_to_its_nearest_code() {
        // Each code alone encodes to itself.
        assert!((0..=255).all(|code| Srgb::encode(u64::from(Srgb::light(code)), 1) == code));
        // Black and white in equal parts are light ½, which sRGB encodes as 0.73536 · 255 =
        // 187.52, so 188, not the 128 a mean of the codes gives; one part black to two of white
        // is light ⅔, 0.83601 · 255 = 213.18, so 213.
        assert_eq!(Srgb::encode(2 * u64::from(SCALE), 4), 188);
        assert_eq!(Srgb::encode(2 * u64::from(SCALE), 3), 213);
    }
}
