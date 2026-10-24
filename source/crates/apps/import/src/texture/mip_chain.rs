use crate::texture::srgb;

/// An 8-bit RGBA texture's full chain of levels, from its base down to 1 × 1: each level a box
/// filter of the one above it, in linear light. A side of `n` texels halves to `max(1, n / 2)`,
/// and each texel of the half covers an equal share, `n / (n / 2)`, of the side above, so an odd
/// side's middle texels are shared by weight and none is dropped. Each level is filtered from the
/// linear values of the one above, kept at `srgb::light`'s scale, so a level's codes round once,
/// not once for each level above it, as a chained box filter equals one over the base but for the
/// rounding between levels.
#[derive(Debug)]
pub(crate) struct MipChain {
    pub(crate) levels: Vec<Vec<u8>>,
}

/// One texel of a side below: the texels above it, each with its weight, in units where a texel
/// above is the count of texels below.
#[derive(Debug)]
struct Share {
    texels: Vec<Weighted>,
}

#[derive(Debug, Clone, Copy)]
struct Weighted {
    at: usize,
    weight: u64,
}

/// A level below: its codes, and its linear values that the level under it reads.
#[derive(Debug)]
struct Level {
    codes: Vec<u8>,
    linear: Vec<u32>,
}

impl MipChain {
    /// The steps of a linear alpha in one of its codes: alpha is linear, so its codes keep 8 bits
    /// more between levels, as color keeps `srgb::light`'s 16.
    const ALPHA_STEPS: u32 = 256;

    /// The chain of `base`, `width` by `height` texels of red, green, blue and alpha, rows from
    /// the top.
    pub(crate) fn of(width: u32, height: u32, base: Vec<u8>) -> Self {
        let (mut width, mut height) = (
            usize::try_from(width).expect("a u32 fits usize"),
            usize::try_from(height).expect("a u32 fits usize"),
        );
        debug_assert_eq!(base.len(), width * height * 4);
        let mut linear: Vec<u32> = base
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|&[r, g, b, a]| {
                [
                    srgb::light(r),
                    srgb::light(g),
                    srgb::light(b),
                    u32::from(a) * Self::ALPHA_STEPS,
                ]
            })
            .collect();
        let mut levels = vec![base];
        while width > 1 || height > 1 {
            let next = Self::down(width, height, &linear);
            levels.push(next.codes);
            linear = next.linear;
            (width, height) = ((width / 2).max(1), (height / 2).max(1));
        }
        Self { levels }
    }

    /// The level below one of `width` by `height` texels of linear values: each texel the weighted
    /// mean of the texels it covers, its color encoded to the nearest code and its alpha rounded
    /// half up, and kept linear, rounded half up, for the level below.
    fn down(width: usize, height: usize, above: &[u32]) -> Level {
        let columns = Self::shares(width);
        let rows = Self::shares(height);
        // Each side's weights sum to its count of texels above.
        let total = u64::try_from(width * height).expect("a usize fits u64");
        let texels = columns.len() * rows.len();
        let mut codes = Vec::with_capacity(texels * 4);
        let mut linear = Vec::with_capacity(texels * 4);
        for row in &rows {
            for column in &columns {
                let mut sums = [0_u64; 4];
                for y in &row.texels {
                    for x in &column.texels {
                        let weight = y.weight * x.weight;
                        let texel = &above[(y.at * width + x.at) * 4..][..4];
                        for (sum, &value) in sums.iter_mut().zip(texel) {
                            *sum += weight * u64::from(value);
                        }
                    }
                }
                let [r, g, b, a] = sums;
                codes.extend([r, g, b].map(|sum| srgb::encode(sum, total)));
                let alpha_total = total * u64::from(Self::ALPHA_STEPS);
                codes.push(
                    u8::try_from((a + alpha_total / 2) / alpha_total).expect("a mean of bytes"),
                );
                linear.extend(sums.map(|sum| {
                    u32::try_from((sum + total / 2) / total).expect("a mean of linear values")
                }));
            }
        }
        Level { codes, linear }
    }

    /// The shares of a side of `above` texels: texel `i` below covers `[i · above, (i + 1) ·
    /// above)` in units where texel `j` above spans `[j · below, (j + 1) · below)`.
    fn shares(above: usize) -> Vec<Share> {
        let below = (above / 2).max(1);
        (0..below)
            .map(|i| {
                let (start, end) = (i * above, (i + 1) * above);
                let texels = (start / below..end.div_ceil(below))
                    .map(|j| Weighted {
                        at: j,
                        weight: u64::try_from(end.min((j + 1) * below) - start.max(j * below))
                            .expect("a usize fits u64"),
                    })
                    .collect();
                Share { texels }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weights(above: usize) -> Vec<Vec<(usize, u64)>> {
        MipChain::shares(above)
            .iter()
            .map(|share| share.texels.iter().map(|t| (t.at, t.weight)).collect())
            .collect()
    }

    #[test]
    fn a_side_halves_into_equal_shares() {
        // 4 → 2: two texels each, of weight 2; 1 → 1: itself; 3 → 1: all three; 5 → 2: in fifths
        // of a texel below, 2, 2 and half of the middle, 1, then 1, 2, 2.
        assert_eq!(weights(4), [vec![(0, 2), (1, 2)], vec![(2, 2), (3, 2)]]);
        assert_eq!(weights(1), [vec![(0, 1)]]);
        assert_eq!(weights(3), [vec![(0, 1), (1, 1), (2, 1)]]);
        assert_eq!(
            weights(5),
            [vec![(0, 2), (1, 2), (2, 1)], vec![(2, 1), (3, 2), (4, 2)]]
        );
    }

    #[test]
    fn a_chain_averages_light_down_to_one_texel() {
        // 4 × 2 texels: a black and white checker on the left, with alpha 0 and 255, and grey
        // 128 on the right, alpha 101. Below, 2 × 1: the left in equal parts black and white,
        // light ½, code 188, alpha 127.5 rounded half up, 128; the right grey and alpha 101.
        // Then 1 × 1, from the light above, not its codes: ½ = 32768 and code 128's 14147 in
        // steps of 2⁻¹⁶, their mean 0.35793, which encodes as 161.32, so 161, where code 188's
        // light, 0.50288, would give 162; alpha (127.5 + 101) / 2 = 114.25, so 114, where code
        // 128's would give 114.5, so 115.
        let (b, w, g) = ([0, 0, 0, 0], [255, 255, 255, 255], [128, 128, 128, 101]);
        let base = [b, w, g, g, w, b, g, g].concat();
        let chain = MipChain::of(4, 2, base.clone());
        assert_eq!(chain.levels.len(), 3);
        assert_eq!(chain.levels[0], base);
        assert_eq!(chain.levels[1], [188, 188, 188, 128, 128, 128, 128, 101]);
        assert_eq!(chain.levels[2], [161, 161, 161, 114]);
    }
}
