use campfire_codec::Toml;

use std::num::NonZeroU8;

use campfire_common::DeclaredName;

use super::*;
use crate::hud_file::panel_grid::{ButtonLook, GridCell};
use crate::keys_file::Key;
use crate::panel_place::PanelCommand;

#[test]
fn a_hud_shows_every_unit_s_bars_and_hits_unless_its_file_says_otherwise() {
    let selected = HudFile {
        bars: ShownBars::Selected,
        hit_mark: HitMark::Ring,
        panel: PanelGrid::default(),
        minimap: Minimap::default(),
        level_icons: Vec::new(),
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
        ..selected.clone()
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
        "unknown field `rings`, expected one of `bars`, `hit_mark`, `panel`, `minimap`, `level_icons`"
    );
}

#[test]
fn a_panel_grid_places_its_buttons_in_its_order_and_holds_each_commands_look() {
    let place = |place: u8| NonZeroU8::new(place).unwrap();
    let cell = |column, row| Some(GridCell { column, row });
    // The default, 5 columns of 3 rows across each row: place 1 the top left, 6 the second row's
    // first, 15 the last, and 16 past it.
    let grid = PanelGrid::default();
    assert_eq!(
        [1, 5, 6, 15].map(|at| grid.cell(place(at))),
        [cell(0, 0), cell(4, 0), cell(0, 1), cell(4, 2)]
    );
    assert_eq!(grid.cell(place(16)), None);
    // Zero Hour's, 7 columns of 2 down each column: place 2 under 1, 3 the second column's top,
    // 14 the last.
    let text = r#"
        [panel]
        columns = 7
        rows = 2
        order = "columns"
        [panel.commands.sell]
        icon = { texture = "client/textures/ui.ktx2", rect = [187, 1, 247, 49] }
        key = "KeyS"
        [panel.commands.stop]
        "#;
    let hud = Toml::parse::<HudFile>(text).unwrap();
    let grid = &hud.panel;
    assert_eq!(
        [1, 2, 3, 14].map(|at| grid.cell(place(at))),
        [cell(0, 0), cell(0, 1), cell(1, 0), cell(6, 1)]
    );
    assert_eq!(grid.cell(place(15)), None);
    let sell = &grid.commands[&PanelCommand::Sell];
    let icon = sell.icon.as_ref().unwrap();
    assert_eq!(
        (icon.texture.as_str(), icon.rect.edges(), sell.key),
        (
            "client/textures/ui.ktx2",
            [187, 1, 247, 49],
            Some(Key::KeyS)
        )
    );
    assert_eq!(grid.commands[&PanelCommand::Stop], ButtonLook::default());
    assert_eq!(Toml::parse::<HudFile>(&Toml::write(&hud)).unwrap(), hud);
    // An icon of no pixel across, or a key of no name, fails to read.
    let refused =
        |icon: &str| Toml::parse::<HudFile>(&format!("[panel.commands.sell]\n{icon}")).is_err();
    assert!(refused(
        r#"icon = { texture = "client/textures/ui.ktx2", rect = [5, 1, 5, 9] }"#
    ));
    assert!(refused(r#"key = "KeyFoo""#));
}

#[test]
fn a_minimap_gives_its_radar_cells_darkening_and_squares_and_refuses_what_does_not_hold() {
    // Zero Hour's: shown by a unit of the tag `radar`, 128 cells, 60 % darker at the lowest ground,
    // squares of 2 cells.
    let text = "[minimap]\nradar = \"radar\"\ncells = 128\ndark = 60\ndot = 2";
    let minimap = Toml::parse::<HudFile>(text).unwrap().minimap;
    assert_eq!(
        minimap,
        Minimap {
            radar: Some(DeclaredName::new("radar").unwrap()),
            ..Minimap::default()
        }
    );
    assert_eq!(Toml::parse::<HudFile>("").unwrap().minimap.radar, None);
    for wrong in ["dark = 101", "cells = 2\ndot = 3"] {
        let text = format!("[minimap]\n{wrong}");
        assert!(Toml::parse::<HudFile>(&text).is_err(), "{wrong}");
    }
}

#[test]
fn a_level_from_2_shows_its_icon_and_level_1_none() {
    // Two icons, of levels 2 and 3, each a rectangle of one texture; level 1 and level 4 show none.
    let text = "[[level_icons]]\ntexture = \"art/veteran.png\"\nrect = [0, 0, 16, 16]\n\n\
                [[level_icons]]\ntexture = \"art/veteran.png\"\nrect = [16, 0, 32, 16]\n";
    let hud = Toml::parse::<HudFile>(text).unwrap();
    assert_eq!(hud.level_icons.len(), 2);
    assert_eq!(hud.level_icon(1), None);
    assert_eq!(hud.level_icon(2), Some(&hud.level_icons[0]));
    assert_eq!(hud.level_icon(3), Some(&hud.level_icons[1]));
    assert_eq!(hud.level_icon(4), None);
    assert_eq!(Toml::parse::<HudFile>(&Toml::write(&hud)).unwrap(), hud);
}
