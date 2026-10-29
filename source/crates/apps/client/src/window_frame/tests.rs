use bevy::asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension};

use super::*;

/// The report's test reads a BGRA frame's exact pixels; each format reads as Bevy reads it,
/// and a 2 × 1 frame of 4 bytes, short of its 8, or of none is refused.
#[test]
fn a_frame_of_bgra_or_rgba_reads_as_bevy_converts_it() {
    let size = Extent3d {
        width: 2,
        ..Extent3d::default()
    };
    for format in [TextureFormat::Bgra8UnormSrgb, TextureFormat::Rgba8UnormSrgb] {
        let pixels = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let usage = RenderAssetUsages::MAIN_WORLD;
        let frame = Image::new(size, TextureDimension::D2, pixels, format, usage);
        let bevy = frame.clone().try_into_dynamic().unwrap().into_rgba8();
        assert_eq!(rgba(&frame).unwrap().pixels, bevy.into_raw(), "{format:?}");
        let refused = |data| {
            rgba(&Image {
                data,
                ..frame.clone()
            })
            .unwrap_err()
        };
        let short = refused(Some(vec![1, 2, 3, 4]));
        assert!(matches!(
            short,
            IntoDynamicImageError::UnknownConversionError(_)
        ));
        let none = refused(None);
        assert!(matches!(none, IntoDynamicImageError::UninitializedImage));
    }
}
