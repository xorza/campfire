use campfire_client_files::IconRect;

use super::*;

#[test]
fn each_drawn_texture_goes_once_and_two_that_share_a_file_clash() {
    // A texture drawn twice goes once; a DDS and a TGA of one name in one directory both go to
    // `x.ktx2`, which the import refuses, naming both.
    let path = ArchivePath::of;
    let files =
        GameWrite::texture_files([path("art\\a.dds"), path("art\\b.tga"), path("art\\a.dds")])
            .unwrap();
    let at = |text: &str| PackagePath::parse(text).unwrap();
    assert_eq!(
        files.into_iter().collect::<Vec<_>>(),
        [
            (at("client/textures/art/a.ktx2"), path("art\\a.dds")),
            (at("client/textures/art/b.ktx2"), path("art\\b.tga")),
        ]
    );
    let clash = GameWrite::texture_files([path("art\\x.dds"), path("art\\x.tga")]).unwrap_err();
    assert!(matches!(
        clash,
        ZeroHourError::TextureClash { first, second }
            if first == path("art\\x.dds") && second == path("art\\x.tga")
    ));
}

#[test]
fn the_power_bar_is_the_game_datas_of_its_four_icons_or_none() {
    let icon = |left| PanelIcon {
        texture: PackagePath::parse("art/bar.ktx2").unwrap(),
        rect: IconRect::new(left, 0, left + 6, 6).unwrap(),
    };
    let icons = [icon(0), icon(6), icon(12), icon(18)];
    // The game's defaults: base 7, 3 intervals, a yellow range of 5.
    let mut game = ZeroHourRules::default();
    let bar = GameWrite::power_bar(&game, &icons).unwrap();
    assert_eq!(
        (bar.base(), bar.intervals(), bar.margin()),
        (7, Num::int(3), 5)
    );
    assert_eq!(bar.fills().near, icon(6));
    assert_eq!(bar.needle(), &icon(18));
    // Three icons, a base of 1 or past a byte, or a range below 0 give none.
    assert_eq!(GameWrite::power_bar(&game, &icons[..3]), None);
    game.game_data.power_bar.base = 1;
    assert_eq!(GameWrite::power_bar(&game, &icons), None);
    game.game_data.power_bar.base = 300;
    assert_eq!(GameWrite::power_bar(&game, &icons), None);
    game.game_data.power_bar.base = 7;
    game.game_data.power_bar.yellow_range = -1;
    assert_eq!(GameWrite::power_bar(&game, &icons), None);
}
