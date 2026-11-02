use std::io::Cursor;
use std::num::NonZeroU32;

use crate::error::{PngError, RgbaImageError};

/// The PNG format of an image of 8-bit RGBA pixels, as a report's screenshot writes it
/// (02-engine-core.md, The screenshot is the frame the window shows).
#[derive(Debug)]
pub struct Png;

/// An image of 8-bit RGBA pixels, row by row from the top: its width and its height in pixels,
/// each one at least, and its pixels, four bytes each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    width: NonZeroU32,
    height: NonZeroU32,
    pixels: Vec<u8>,
}

impl RgbaImage {
    /// The image of `width` by `height` pixels of `pixels`; an error for a size of no pixel, or
    /// pixels of another length than four bytes of each.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, RgbaImageError> {
        let (Some(width), Some(height)) = (NonZeroU32::new(width), NonZeroU32::new(height)) else {
            return Err(RgbaImageError::Empty);
        };
        let expected = u64::from(width.get()) * u64::from(height.get()) * 4;
        if u64::try_from(pixels.len()) != Ok(expected) {
            return Err(RgbaImageError::Pixels {
                expected,
                found: pixels.len(),
            });
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }

    pub const fn width(&self) -> u32 {
        self.width.get()
    }

    pub const fn height(&self) -> u32 {
        self.height.get()
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

impl Png {
    /// The PNG file of `image`, compressed fast, as a frame of a large window takes tens of
    /// megabytes raw.
    pub fn encode(image: &RgbaImage) -> Vec<u8> {
        let mut file = Vec::new();
        let mut encoder = png::Encoder::new(&mut file, image.width(), image.height());
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder
            .write_header()
            .expect("a header of a size above none writes into memory");
        writer
            .write_image_data(&image.pixels)
            .expect("an image of its size writes into memory");
        writer.finish().expect("a whole image ends in memory");
        file
    }

    /// The image of the PNG file `bytes`, of 8-bit RGBA pixels; an error for a file that does
    /// not decode, or holds pixels of another kind.
    pub fn decode(bytes: &[u8]) -> Result<RgbaImage, PngError> {
        let decoder = png::Decoder::new(Cursor::new(bytes));
        let mut reader = decoder.read_info()?;
        let info = reader.info();
        if (info.color_type, info.bit_depth) != (png::ColorType::Rgba, png::BitDepth::Eight) {
            return Err(PngError::NotRgba);
        }
        let (width, height) = (info.width, info.height);
        let size = reader.output_buffer_size().ok_or(PngError::NotRgba)?;
        let mut pixels = vec![0; size];
        reader.next_frame(&mut pixels)?;
        Ok(RgbaImage::new(width, height, pixels)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_image_writes_as_a_png_and_reads_back_whole() {
        // Two by two: red, green, blue, and a white half seen through.
        let image = RgbaImage::new(
            2,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 128,
            ],
        )
        .unwrap();
        // A size of no pixel, or pixels short of four bytes of each, is no image.
        assert_eq!(RgbaImage::new(0, 2, Vec::new()), Err(RgbaImageError::Empty));
        let short = RgbaImage::new(2, 2, vec![0; 15]);
        let found = RgbaImageError::Pixels {
            expected: 16,
            found: 15,
        };
        assert_eq!(short, Err(found));
        let file = Png::encode(&image);
        assert_eq!(&file[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(Png::decode(&file).unwrap(), image);
        assert!(matches!(
            Png::decode(b"not a png"),
            Err(PngError::Decode(png::DecodingError::Format(_)))
        ));
    }
}
