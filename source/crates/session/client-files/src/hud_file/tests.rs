use campfire_codec::Toml;

use super::*;

#[test]
fn a_hud_shows_every_unit_s_bars_unless_its_file_says_the_selected() {
    let selected = HudFile {
        bars: ShownBars::Selected,
    };
    assert_eq!(
        Toml::parse::<HudFile>("bars = \"selected\"").unwrap(),
        selected
    );
    assert_eq!(Toml::parse::<HudFile>("").unwrap().bars, ShownBars::All);
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
        "unknown field `rings`, expected `bars`"
    );
}
