use crate::texture::srgb::Srgb;

/// An 8-bit RGBA texture's full chain of levels, from its base down to 1 × 1: each level a box
/// filter of the one above it, in linear light. A side of `n` texels halves to `max(1, n / 2)`,
/// and each texel of the half covers an equal share, `n / (n / 2)`, of the side above, so an odd
/// side's middle texels are shared by weight and none is dropped.
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

impl MipChain {
    /// The chain of `base`, `width` by `height` texels of red, green, blue and alpha, rows from
    /// the top.
    pub(crate) fn of(width: u32, height: u32, base: Vec<u8>) -> MipChain {
        debug_assert_eq!(base.len(), width as usize * height as usize * 4);
        let mut levels = vec![base];
        let (mut width, mut height) = (width as usize, height as usize);
        while width > 1 || height > 1 {
            let next = MipChain::down(width, height, levels.last().expect("a base"));
            levels.push(next);
            (width, height) = ((width / 2).max(1), (height / 2).max(1));
        }
        MipChain { levels }
    }

    /// The level below one of `width` by `height`: each texel the weighted mean of the texels it
    /// covers, its color in linear light encoded back to the nearest code, its alpha rounded half
    /// up.
    fn down(width: usize, height: usize, above: &[u8]) -> Vec<u8> {
        let columns = MipChain::shares(width);
        let rows = MipChain::shares(height);
        // Each side's weights sum to its count of texels above.
        let total = (width * height) as u64;
        let mut below = Vec::with_capacity(columns.len() * rows.len() * 4);
        for row in &rows {
            for column in &columns {
                let mut light = [0_u64; 3];
                let mut alpha = 0_u64;
                for y in &row.texels {
                    for x in &column.texels {
                        let weight = y.weight * x.weight;
                        let texel = &above[(y.at * width + x.at) * 4..][..4];
                        for (sum, &code) in light.iter_mut().zip(&texel[..3]) {
                            *sum += weight * u64::from(Srgb::light(code));
                        }
                        alpha += weight * u64::from(texel[3]);
                    }
                }
                below.extend(light.map(|sum| Srgb::encode(sum, total)));
                below.push(u8::try_from((alpha + total / 2) / total).expect("a mean of bytes"));
            }
        }
        below
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
                        weight: (end.min((j + 1) * below) - start.max(j * below)) as u64,
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
        // 128 on the right, alpha 100. Below, 2 × 1: the left in equal parts black and white,
        // light ½, code 188, alpha 127.5 rounded half up, 128; the right grey and alpha 100.
        // Then 1 × 1: codes 188 and 128 are light 0.50288 and 0.21587, their mean 0.35938, which
        // encodes as 161.61, so 162; alpha (128 + 100) / 2 = 114.
        let (b, w, g) = ([0, 0, 0, 0], [255, 255, 255, 255], [128, 128, 128, 100]);
        let base = [b, w, g, g, w, b, g, g].concat();
        let chain = MipChain::of(4, 2, base.clone());
        assert_eq!(chain.levels.len(), 3);
        assert_eq!(chain.levels[0], base);
        assert_eq!(chain.levels[1], [188, 188, 188, 128, 128, 128, 128, 100]);
        assert_eq!(chain.levels[2], [162, 162, 162, 114]);
    }
}
