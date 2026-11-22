use campfire_client_files::{IconRect, PowerFills};
use campfire_math::Num;
use campfire_values::PackagePath;

use super::*;

/// Zero Hour's bar: base 7, 3 intervals, a margin of 5; its tiles and needle of no matter here.
fn bar() -> PowerBar {
    let icon = || PanelIcon {
        texture: PackagePath::parse("art/bar.ktx2").unwrap(),
        rect: IconRect::new(0, 0, 6, 6).unwrap(),
    };
    let fills = PowerFills {
        enough: icon(),
        near: icon(),
        short: icon(),
    };
    PowerBar::new(7, Num::int(3), 5, fills, icon()).unwrap()
}

/// A bar 300 px wide, of tiles of 6 and a needle of 11: 100 px an interval, a power of 7.
const WIDTHS: BarWidths = BarWidths {
    bar: 300.0,
    tile: 6.0,
    needle: 11.0,
};

fn draw(given: i64, used: i64) -> BarDraw {
    let power = PlayerPower::new(Num::int(given), Num::int(used));
    BarDraw::of(&bar(), power, WIDTHS)
}

/// Whether `got` is `want` within a thousandth of a pixel, the rounding of `f32`'s logarithms,
/// far below a pixel a bar draws.
fn near(got: f32, want: f32) -> bool {
    (got - want).abs() < 1e-3
}

#[test]
fn a_bar_draws_its_fill_by_whole_tiles_and_one_more_and_its_needle_at_the_use() {
    // 49 of 7: log₇ 49 = 2 intervals, 200 px; 33 whole tiles, 198, and one more, 204. The use of
    // 7, 1 interval, 100 px, its needle 5.5 to the left, 94.5; 7 at most 49 − 5: enough.
    let enough = draw(49, 7);
    assert!(near(enough.fill, 204.0), "{enough:?}");
    assert!(near(enough.needle.unwrap(), 94.5), "{enough:?}");
    assert_eq!(enough.state, PowerState::Enough);
    // 3 of 7 lies within 5 below it: near. 7 is 100 px, 16 tiles and one, 102; log₇ 3 =
    // 0.564575, 56.4575 px, its needle at 50.9575.
    let near_draw = draw(7, 3);
    assert!(near(near_draw.fill, 102.0), "{near_draw:?}");
    assert!(near(near_draw.needle.unwrap(), 50.9575), "{near_draw:?}");
    assert_eq!(near_draw.state, PowerState::Near);
    // 6 of 5: short. log₇ 5 = 0.827087, 82.7087 px, 13 tiles and one, 84; log₇ 6 = 0.920782,
    // its needle at 92.0782 − 5.5 = 86.5782.
    let short = draw(5, 6);
    assert!(near(short.fill, 84.0), "{short:?}");
    assert!(near(short.needle.unwrap(), 86.5782), "{short:?}");
    assert_eq!(short.state, PowerState::Short);
    // No power and no use: no fill, no needle, and not short. A use of 1 stands at log₇ 1.5 =
    // 0.208370, 20.837 px, as log₇ 1 would show nothing: 15.337.
    let none = draw(0, 0);
    assert_eq!(
        (none.fill, none.needle, none.state),
        (0.0, None, PowerState::Near)
    );
    let one = draw(0, 1);
    assert_eq!(one.fill, 0.0);
    assert!(near(one.needle.unwrap(), 15.337), "{one:?}");
    assert_eq!(one.state, PowerState::Short);
    // 7⁴ is past the bar's 3 intervals: the whole bar, 50 tiles and no more; a use of 7³ stands
    // at its end, 300 − 11.
    let full = draw(2401, 343);
    assert!(near(full.fill, 300.0), "{full:?}");
    assert!(near(full.needle.unwrap(), 289.0), "{full:?}");
}
