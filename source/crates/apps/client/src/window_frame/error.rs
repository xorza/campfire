use bevy::image::IntoDynamicImageError;
use campfire_codec::RgbaImageError;
use thiserror::Error;

/// Why the window's frame gives no image: Bevy does not convert it, or it holds no pixel.
#[derive(Debug, Error)]
pub(crate) enum FrameError {
    #[error(transparent)]
    Convert(#[from] IntoDynamicImageError),
    #[error(transparent)]
    Image(#[from] RgbaImageError),
}
