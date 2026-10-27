use bevy::math::{URect, UVec2, Vec2};
use campfire_codec::RgbaImage;

/// A screenshot's region of the window's frame, in the frame's pixels, and the size of the image
/// it gives: the largest whose sides pass no 2,000 px and whose visual tokens,
/// `⌈width / 28⌉ × ⌈height / 28⌉`, pass no 4,784, so Claude reads it at its own pixels and
/// names a point in them (console.md, A pixel is the screenshot's).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShotFrame {
    region: URect,
    image: UVec2,
}

impl ShotFrame {
    /// The most pixels of an image's side.
    const MOST_SIDE: u32 = 2000;
    /// The most visual tokens of an image.
    const MOST_TOKENS: u32 = 4784;
    /// The pixels of a visual token's side.
    const TOKEN_SIDE: u32 = 28;
    /// The pixels a box round units leaves about them.
    const MARGIN: f32 = 16.0;

    /// The shot of `region` of a frame of `frame` pixels, cut to the frame; none for a region of
    /// no pixel within it.
    pub(crate) fn of(region: URect, frame: UVec2) -> Option<Self> {
        let region = region.intersect(URect::from_corners(UVec2::ZERO, frame));
        if region.is_empty() {
            return None;
        }
        let size = region.size();
        let side = size.x.max(size.y);
        let mut width = if side > Self::MOST_SIDE {
            u32::try_from(u64::from(Self::MOST_SIDE) * u64::from(size.x) / u64::from(side))
                .expect("a width below the region's")
        } else {
            size.x
        };
        loop {
            let height = Self::height_of(width, size);
            let tokens = width.div_ceil(Self::TOKEN_SIDE) * height.div_ceil(Self::TOKEN_SIDE);
            if tokens <= Self::MOST_TOKENS || width == 1 {
                return Some(Self {
                    region,
                    image: UVec2::new(width, height),
                });
            }
            width -= 1;
        }
    }

    /// The height of an image `width` wide of a region of `size`, its aspect kept, at least 1.
    fn height_of(width: u32, size: UVec2) -> u32 {
        let height = u64::from(width) * u64::from(size.y) / u64::from(size.x);
        u32::try_from(height)
            .expect("a height below the region's")
            .max(1)
    }

    pub(crate) const fn region(&self) -> URect {
        self.region
    }

    /// The image's pixels a frame's pixel of the region.
    pub(crate) fn scale(&self) -> f32 {
        self.image.x as f32 / self.region.width() as f32
    }

    /// The frame's pixels the rectangle `pixels` of this shot's image covers.
    pub(crate) fn frame_rect(&self, pixels: URect) -> URect {
        let size = self.region.size();
        let back = |at: UVec2, round_up: bool| {
            let axis = |at: u32, of: u32, image: u32| {
                let scaled = u64::from(at) * u64::from(of);
                let at = if round_up {
                    scaled.div_ceil(u64::from(image))
                } else {
                    scaled / u64::from(image)
                };
                u32::try_from(at).expect("a pixel of the region").min(of)
            };
            self.region.min
                + UVec2::new(
                    axis(at.x, size.x, self.image.x),
                    axis(at.y, size.y, self.image.y),
                )
        };
        URect::from_corners(back(pixels.min, false), back(pixels.max, true))
    }

    /// The image of the region of `frame`, each of its pixels the mean of the frame's pixels it
    /// covers.
    pub(crate) fn image(&self, frame: &RgbaImage) -> RgbaImage {
        let size = self.region.size();
        let mut pixels = Vec::with_capacity(self.image.x as usize * self.image.y as usize * 4);
        let span = |at: u32, of: u32, image: u32| {
            let start = u64::from(at) * u64::from(of) / u64::from(image);
            let end = (u64::from(at) + 1) * u64::from(of) / u64::from(image);
            let start = u32::try_from(start).expect("a pixel of the region");
            let end = u32::try_from(end)
                .expect("a pixel of the region")
                .max(start + 1);
            start..end
        };
        for y in 0..self.image.y {
            let rows = span(y, size.y, self.image.y);
            for x in 0..self.image.x {
                let columns = span(x, size.x, self.image.x);
                let mut sum = [0_u64; 4];
                for row in rows.clone() {
                    for column in columns.clone() {
                        let at = ((self.region.min.y + row) as usize * frame.width as usize
                            + (self.region.min.x + column) as usize)
                            * 4;
                        for (sum, &byte) in sum.iter_mut().zip(&frame.pixels[at..at + 4]) {
                            *sum += u64::from(byte);
                        }
                    }
                }
                let count = u64::from(rows.len() as u32 * columns.len() as u32);
                pixels.extend(
                    sum.map(|sum| {
                        u8::try_from((sum + count / 2) / count).expect("a mean of bytes")
                    }),
                );
            }
        }
        RgbaImage {
            width: self.image.x,
            height: self.image.y,
            pixels,
        }
    }

    /// The box round `points`, the frame's pixels a unit's bounds show at, with a margin, within
    /// a frame of `frame` pixels; none for no point.
    pub(crate) fn round(points: impl IntoIterator<Item = Vec2>, frame: UVec2) -> Option<URect> {
        let mut points = points.into_iter();
        let first = points.next()?;
        let (least, most) = points.fold((first, first), |(least, most), point| {
            (least.min(point), most.max(point))
        });
        let least = (least - Self::MARGIN).max(Vec2::ZERO).floor();
        let most = (most + Self::MARGIN).min(frame.as_vec2()).ceil();
        let rect = URect::from_corners(least.as_uvec2(), most.as_uvec2());
        (!rect.is_empty()).then_some(rect)
    }
}

#[cfg(test)]
mod tests;
