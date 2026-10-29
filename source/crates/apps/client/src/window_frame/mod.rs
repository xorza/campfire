//! The window's frame, as a screenshot captures it, in 8-bit RGBA.

use bevy::image::{Image, IntoDynamicImageError};
use bevy::render::render_resource::TextureFormat;
use campfire_codec::RgbaImage;

/// The pixels of `frame` as 8-bit RGBA: a frame of 8-bit BGRA or RGBA, which a window renders
/// to, read in place, one pass and no copy of the frame; any other converted as Bevy converts it,
/// from a copy. A frame of no data, or of other than its size's pixels, is refused as Bevy
/// refuses it.
pub(crate) fn rgba(frame: &Image) -> Result<RgbaImage, IntoDynamicImageError> {
    let format = frame.texture_descriptor.format;
    let swapped = match format {
        TextureFormat::Bgra8Unorm | TextureFormat::Bgra8UnormSrgb => true,
        TextureFormat::Rgba8Unorm | TextureFormat::Rgba8UnormSrgb => false,
        _ => {
            let image = frame.clone().try_into_dynamic()?.into_rgba8();
            return Ok(RgbaImage {
                width: image.width(),
                height: image.height(),
                pixels: image.into_raw(),
            });
        }
    };
    let data = frame
        .data
        .as_deref()
        .ok_or(IntoDynamicImageError::UninitializedImage)?;
    let len = u64::from(frame.width()) * u64::from(frame.height()) * 4;
    if u64::try_from(data.len()) != Ok(len) {
        return Err(IntoDynamicImageError::UnknownConversionError(format));
    }
    let mut pixels = Vec::with_capacity(data.len());
    for &[first, second, third, alpha] in data.as_chunks::<4>().0 {
        let [red, blue] = if swapped {
            [third, first]
        } else {
            [first, third]
        };
        pixels.extend_from_slice(&[red, second, blue, alpha]);
    }
    Ok(RgbaImage {
        width: frame.width(),
        height: frame.height(),
        pixels,
    })
}

#[cfg(test)]
mod tests;
