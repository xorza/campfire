use std::io::Cursor;

use crate::error::PngError;

/// The PNG format of an image of 8-bit RGBA pixels, as a report's screenshot writes it
/// (bug-reports.md, The screenshot is the frame the window shows).
#[derive(Debug)]
pub struct Png;

/// An image of 8-bit RGBA pixels, row by row from the top: its width and its height in pixels,
/// and its pixels, four bytes each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Png {
    /// The PNG file of `image`, compressed fast, as a frame of a large window takes tens of
    /// megabytes raw.
    pub fn encode(image: &RgbaImage) -> Vec<u8> {
        let RgbaImage {
            width,
            height,
            pixels,
        } = image;
        debug_assert_eq!(
            pixels.len() as u64,
            u64::from(*width) * u64::from(*height) * 4,
            "an image holds four bytes of each of its pixels"
        );
        let mut file = Vec::new();
        let mut encoder = png::Encoder::new(&mut file, *width, *height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let mut writer = encoder
            .write_header()
            .expect("a header of a size above none writes into memory");
        writer
            .write_image_data(pixels)
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
        Ok(RgbaImage {
            width,
            height,
            pixels,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_image_writes_as_a_png_and_reads_back_whole() {
        // Two by two: red, green, blue, and a white half seen through.
        let image = RgbaImage {
            width: 2,
            height: 2,
            pixels: vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 128,
            ],
        };
        let file = Png::encode(&image);
        assert_eq!(&file[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(Png::decode(&file).unwrap(), image);
        assert!(matches!(
            Png::decode(b"not a png"),
            Err(PngError::Decode(png::DecodingError::Format(_)))
        ));
    }
}
