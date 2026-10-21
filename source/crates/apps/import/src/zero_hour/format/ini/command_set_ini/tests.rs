use super::*;
use crate::zero_hour::format::ini::command_set_ini::samples::COMMAND_SETS;

#[test]
fn a_set_lists_its_buttons_by_place_a_later_file_over_the_places_it_names() {
    fn buttons<'a>(ini: &'a CommandSetIni, name: &str) -> Option<Vec<&'a str>> {
        Some(ini.buttons(name)?.collect())
    }
    let mut ini = CommandSetIni::default();
    ini.read(COMMAND_SETS.as_bytes()).unwrap();
    // By place, not by the order of the lines.
    assert_eq!(
        buttons(&ini, "AmericaDozerCommandSet").unwrap(),
        [
            "Command_ConstructAmericaPowerPlant",
            "Command_ConstructAmericaBarracks",
            "Command_ConstructAmericaFireBase",
            "Command_ConstructAmericaInfantryRanger"
        ]
    );
    assert!(buttons(&ini, "americadozercommandset").is_none());
    ini.read(b"CommandSet AmericaCommandCenterCommandSet\n 1 = Other\n 2 = Second\nEnd\n")
        .unwrap();
    assert_eq!(
        buttons(&ini, "AmericaCommandCenterCommandSet").unwrap(),
        ["Other", "Second", "Command_SetRallyPoint"]
    );
    let fails = |text: &[u8]| CommandSetIni::default().read(text).unwrap_err();
    assert_eq!(
        fails(b"CommandSet A\n 19 = B\nEnd\n"),
        IniError::NotListed { line: 2 }
    );
    assert_eq!(
        fails(b"CommandSet A\n 0 = B\nEnd\n"),
        IniError::NotListed { line: 2 }
    );
    assert_eq!(
        fails(b"CommandSet A\n First = B\nEnd\n"),
        IniError::NotListed { line: 2 }
    );
    assert_eq!(
        fails(b"CommandSet A\n 1\nEnd\n"),
        IniError::NoValue { line: 2 }
    );
    assert_eq!(fails(b"CommandSet A\n"), IniError::Unclosed);
}
