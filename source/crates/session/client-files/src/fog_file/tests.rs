use campfire_codec::Toml;

use super::*;

#[test]
fn a_fog_file_gives_its_levels_and_color_and_refuses_a_seen_cell_darker_than_a_fogged_one() {
    // Zero Hour's: a seen cell keeps 255, an explored one 127, a shrouded one none, in white.
    let text = "seen = 255\nexplored = 127\nshrouded = 0\ncolor = [255, 255, 255]";
    let file = Toml::parse::<FogFile>(text).unwrap();
    assert_eq!(
        file,
        FogFile {
            seen: FogLevel(255),
            explored: FogLevel(127),
            shrouded: FogLevel(0),
            color: [255; 3],
        }
    );
    assert_eq!(Toml::parse::<FogFile>(&Toml::write(&file)), Ok(file));
    // A light is the color times the level over 255, rounded: a blue fog of 200 at 127 keeps
    // 200 · 127 / 255 = 99.6, 100, of blue, and white keeps 127.
    let blue = FogFile {
        color: [0, 64, 200],
        ..file
    };
    assert_eq!(blue.light(FogLevel(127)), [0, 32, 100]);
    assert_eq!(file.light(FogLevel(127)), [127; 3]);
    assert_eq!(file.light(FogLevel(0)), [0; 3]);
    // An explored cell lighter than a seen one, or a shrouded one than an explored one, fails.
    for text in [
        "seen = 100\nexplored = 127\nshrouded = 0\ncolor = [255, 255, 255]",
        "seen = 255\nexplored = 10\nshrouded = 20\ncolor = [255, 255, 255]",
    ] {
        assert!(Toml::parse::<FogFile>(text).is_err(), "{text}");
    }
    assert_eq!(FogFile::path().as_str(), "client/fog.toml");
}
