use super::*;
use crate::zero_hour::format::csf::string_table::samples::csf;

/// The labels of the buttons: the dozer's marks `d`, the sell's `s`, the stop's a digit, and the
/// guard's nothing.
fn strings() -> StringTable {
    StringTable::read(&csf(&[
        ("CONTROLBAR:Dozer", &[("&Dozer", None)]),
        ("CONTROLBAR:Sell", &[("&Sell", None)]),
        ("CONTROLBAR:Stop", &[("Stop &1", None)]),
        ("CONTROLBAR:Guard", &[("Guard", None)]),
    ]))
    .unwrap()
}

fn button(command: ButtonCommand, label: &str, image: Option<&str>) -> CommandButton {
    CommandButton {
        command,
        label: Some(label.to_owned()),
        image: image.map(str::to_owned),
        ..CommandButton::default()
    }
}

#[test]
fn a_set_places_its_converted_buttons_and_commands_with_their_images_and_keys() {
    let strings = strings();
    let center = TypeName::named("americacommandcenter");
    let dozer = DeclaredName::new("train_americavehicledozer").unwrap();
    let mut buttons = PanelButtons::default();
    let mut place = |at: u8, name: &str, button: &CommandButton, action: Option<&DeclaredName>| {
        buttons.place(&strings, &center, (at, name), button, action.cloned());
    };
    let train = button(
        ButtonCommand::UnitBuild,
        "CONTROLBAR:Dozer",
        Some("SACDozer"),
    );
    let sell = button(ButtonCommand::Sell, "CONTROLBAR:Sell", Some("SSSell2"));
    let stop = button(ButtonCommand::Stop, "CONTROLBAR:Stop", Some("Turned"));
    let guard = button(ButtonCommand::Other, "CONTROLBAR:Guard", None);
    // The dozer's train at 1, the sell at 14, the stop at 13, a power at 2 the engine holds no
    // order for, a dozer's train again past 14, and a train the import dropped at 3.
    place(1, "Command_ConstructAmericaDozer", &train, Some(&dozer));
    place(14, "Command_Sell", &sell, None);
    place(13, "Command_Stop", &stop, None);
    place(2, "Command_Guard", &guard, None);
    place(15, "Command_ConstructAmericaDozer", &train, Some(&dozer));
    place(3, "Command_ConstructGhost", &train, None);
    let at = |place: u8, button| PanelPlace {
        place: NonZeroU8::new(place).unwrap(),
        button,
    };
    assert_eq!(
        buttons.places[&center],
        [
            at(1, PanelButton::Action(dozer.clone())),
            at(14, PanelButton::Command(PanelCommand::Sell)),
            at(13, PanelButton::Command(PanelCommand::Stop)),
        ]
    );
    // The dozer's face, its key the letter after its `&`, and the stop's a digit's.
    assert_eq!(
        buttons.actions[&dozer],
        ButtonFace {
            button: "Command_ConstructAmericaDozer".to_owned(),
            image: Some("SACDozer".to_owned()),
            key: Some(Key::KeyD),
        }
    );
    assert_eq!(buttons.commands[&PanelCommand::Stop].key, Some(Key::Digit1));
    let causes: Vec<(String, PanelDropCause)> = (&buttons.dropped)
        .into_iter()
        .map(|dropped| (dropped.name.clone(), dropped.cause))
        .collect();
    assert_eq!(
        causes,
        [
            ("Command_Guard".to_owned(), PanelDropCause::Command),
            (
                "Command_ConstructAmericaDozer".to_owned(),
                PanelDropCause::Place
            ),
        ]
    );

    // The images: the dozer's of its atlas, the sell's of a texture no archive holds, the stop's
    // turned; each converted once.
    let mut images = MappedImageIni::default();
    images
        .read(
            b"MappedImage SACDozer\n Texture = SAUserInterface512_005.tga\n \
              Coords = Left:63 Top:301 Right:123 Bottom:349\nEnd\n\
              MappedImage SSSell2\n Texture = Lost.tga\n Coords = Left:1 Top:1 Right:9 Bottom:9\nEnd\n\
              MappedImage Turned\n Texture = SAUserInterface512_005.tga\n \
              Coords = Left:0 Top:0 Right:9 Bottom:9\n Status = ROTATED_90_CLOCKWISE\nEnd\n",
        )
        .unwrap();
    let atlas = ArchivePath::of("art\\textures\\sauserinterface512_005.tga");
    let texture = |name: &str| (name == "SAUserInterface512_005.tga").then(|| atlas.clone());
    let mut dropped = Losses::default();
    let panel = PanelImport::of(&buttons, &images, texture, &mut dropped).unwrap();
    assert_eq!(
        panel.actions[&dozer],
        ButtonLook {
            icon: Some(PanelIcon {
                texture: game_path::texture(&atlas).unwrap(),
                rect: IconRect::new(63, 301, 123, 349).unwrap(),
            }),
            key: Some(Key::KeyD),
        }
    );
    let sold = &panel.grid.commands[&PanelCommand::Sell];
    assert_eq!((&sold.icon, sold.key), (&None, Some(Key::KeyS)));
    assert_eq!(panel.grid.commands[&PanelCommand::Stop].icon, None);
    assert_eq!(panel.textures, BTreeSet::from([atlas]));
    // The game's grid: 7 columns of 2, down each column.
    let grid = &panel.grid;
    assert_eq!(
        (grid.columns.get(), grid.rows.get(), grid.order),
        (7, 2, PlaceOrder::Columns)
    );
    let causes: Vec<(String, PanelDropCause)> = (&dropped)
        .into_iter()
        .map(|dropped| (dropped.name.clone(), dropped.cause))
        .collect();
    assert_eq!(
        causes,
        [
            ("Command_Sell".to_owned(), PanelDropCause::NoTexture),
            ("Command_Stop".to_owned(), PanelDropCause::Rotated),
        ]
    );
}
