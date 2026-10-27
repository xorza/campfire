use super::*;

#[test]
fn a_shot_scales_to_both_bounds_of_claudes_images() {
    let size = |region: URect, frame| ShotFrame::of(region, frame).unwrap().image;
    let whole = |w, h| URect::new(0, 0, w, h);
    // 800 × 600 fits: ⌈800 / 28⌉ × ⌈600 / 28⌉ = 29 × 22 = 638 tokens.
    assert_eq!(
        size(whole(800, 600), UVec2::new(800, 600)),
        UVec2::new(800, 600)
    );
    // 7680 × 2160, the 57" monitor: its long side to 2,000, the other 2,160 × 2,000 / 7,680 =
    // 562.5, so 562; 72 × 21 = 1,512 tokens.
    let wide = ShotFrame::of(whole(7680, 2160), UVec2::new(7680, 2160)).unwrap();
    assert_eq!(wide.image, UVec2::new(2000, 562));
    assert_eq!(wide.scale(), 2000.0 / 7680.0);
    // 3000 × 3000: 2,000 a side takes ⌈2,000 / 28⌉² = 72² = 5,184 tokens; 69² = 4,761 is the most
    // square within 4,784, so 69 × 28 = 1,932 a side, and 1,933 takes 70² = 4,900.
    assert_eq!(
        size(whole(3000, 3000), UVec2::new(3000, 3000)),
        UVec2::new(1932, 1932)
    );
    // A region past the frame is cut to it; one wholly past it is none.
    let cut = ShotFrame::of(URect::new(700, 500, 900, 700), UVec2::new(800, 600)).unwrap();
    assert_eq!(cut.region(), URect::new(700, 500, 800, 600));
    assert_eq!(
        ShotFrame::of(URect::new(900, 0, 950, 10), UVec2::new(800, 600)),
        None
    );
}

#[test]
fn a_shot_crops_and_means_the_frames_pixels() {
    // A frame of 4 × 2, each pixel's red its column × 10 and green its row × 100.
    let frame = RgbaImage {
        width: 4,
        height: 2,
        pixels: (0..2_u8)
            .flat_map(|row| (0..4_u8).flat_map(move |column| [column * 10, row * 100, 0, 255]))
            .collect(),
    };
    // Its columns 1 to 2 of row 1, at their own size.
    let crop = ShotFrame::of(URect::new(1, 1, 3, 2), UVec2::new(4, 2)).unwrap();
    assert_eq!(
        crop.image(&frame).pixels,
        [10, 100, 0, 255, 20, 100, 0, 255]
    );
    // The whole frame halved, as a shot of a frame past the bounds would: each pixel the mean of
    // a square of 2 × 2, red (0 + 10 + 0 + 10) / 4 = 5 and green (0 + 0 + 100 + 100) / 4 = 50, then
    // red 25.
    let half = ShotFrame {
        region: URect::new(0, 0, 4, 2),
        image: UVec2::new(2, 1),
    };
    let image = half.image(&frame);
    assert_eq!((image.width, image.height), (2, 1));
    assert_eq!(image.pixels, [5, 50, 0, 255, 25, 50, 0, 255]);
    // A rectangle of the halved image covers twice its pixels of the frame, from the region.
    assert_eq!(
        half.frame_rect(URect::new(1, 0, 2, 1)),
        URect::new(2, 0, 4, 2)
    );
    let offset = ShotFrame {
        region: URect::new(100, 50, 104, 52),
        image: UVec2::new(2, 1),
    };
    assert_eq!(
        offset.frame_rect(URect::new(0, 0, 1, 1)),
        URect::new(100, 50, 102, 52)
    );
}

#[test]
fn a_box_rounds_the_units_points_with_its_margin_within_the_frame() {
    let frame = UVec2::new(800, 600);
    // Points at x 100 to 300 and y 200 to 250, 16 px more each way.
    let points = [
        Vec2::new(100.0, 250.0),
        Vec2::new(300.0, 200.0),
        Vec2::new(150.5, 210.0),
    ];
    assert_eq!(
        ShotFrame::round(points, frame),
        Some(URect::new(84, 184, 316, 266))
    );
    // Near the corner the box stops at the frame; no point, no box.
    assert_eq!(
        ShotFrame::round([Vec2::new(5.0, 590.0)], frame),
        Some(URect::new(0, 574, 21, 600))
    );
    assert_eq!(ShotFrame::round([], frame), None);
}
