use campfire_codec::Toml;

use super::*;

#[test]
fn a_hud_shows_every_unit_s_bars_and_hits_unless_its_file_says_otherwise() {
    let selected = HudFile {
        bars: ShownBars::Selected,
        hit_mark: HitMark::Ring,
    };
    assert_eq!(
        Toml::parse::<HudFile>("bars = \"selected\"").unwrap(),
        selected
    );
    assert_eq!(Toml::parse::<HudFile>("").unwrap(), HudFile::default());
    assert_eq!(
        (HudFile::default().bars, HudFile::default().hit_mark),
        (ShownBars::All, HitMark::Ring)
    );
    let hidden = HudFile {
        hit_mark: HitMark::Hidden,
        ..selected
    };
    let text = "bars = \"selected\"\nhit_mark = \"hidden\"";
    assert_eq!(Toml::parse::<HudFile>(text).unwrap(), hidden);
    assert_eq!(
        Toml::parse::<HudFile>(&Toml::write(&hidden)).unwrap(),
        hidden
    );
    assert_eq!(
        Toml::parse::<HudFile>(&Toml::write(&selected)).unwrap(),
        selected
    );
    let refusal = |text: &str| {
        let refused = Toml::parse::<HudFile>(text).unwrap_err();
        refused.message().to_owned()
    };
    assert_eq!(
        refusal("bars = \"some\""),
        "unknown variant `some`, expected `all` or `selected`"
    );
    assert_eq!(
        refusal("rings = \"all\""),
        "unknown field `rings`, expected `bars` or `hit_mark`"
    );
}
