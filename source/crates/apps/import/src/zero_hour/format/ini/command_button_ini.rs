use std::collections::BTreeMap;

use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{BlockKinds, IniText};

/// The command buttons of Zero Hour's `CommandButton.ini` files, as `ControlBar` reads them, by
/// their names, of their case.
#[derive(Debug, Default)]
pub(crate) struct CommandButtonIni {
    buttons: BTreeMap<String, CommandButton>,
}

/// What the import reads of a button: its command, the object it makes, the upgrade it
/// researches, the label of its text in the string table, its `ButtonImage`, a mapped image's
/// name, the sciences of its `Science` line: those it needs, or for a purchase the one it buys,
/// and the special power it fires, `SpecialPower`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CommandButton {
    pub(crate) command: ButtonCommand,
    pub(crate) object: Option<String>,
    pub(crate) upgrade: Option<String>,
    pub(crate) label: Option<String>,
    pub(crate) image: Option<String>,
    pub(crate) sciences: Vec<String>,
    pub(crate) power: Option<String>,
}

/// A button's `Command`, of those the import converts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum ButtonCommand {
    /// `UNIT_BUILD`: its producer trains its object.
    UnitBuild,
    /// `DOZER_CONSTRUCT`: its builder places its object to build it.
    DozerConstruct,
    /// `PLAYER_UPGRADE`: its producer researches its upgrade for its player.
    PlayerUpgrade,
    /// `OBJECT_UPGRADE`: its producer researches its upgrade for itself.
    ObjectUpgrade,
    /// `SELL`: its building is sold.
    Sell,
    /// `STOP`: its units stop.
    Stop,
    /// `ATTACK_MOVE`: its units attack-move to the point the next click picks.
    AttackMove,
    /// `SET_RALLY_POINT`: its producers rally to the point the next click picks.
    SetRallyPoint,
    /// `PURCHASE_SCIENCE`: its player buys its science.
    PurchaseScience,
    /// `SPECIAL_POWER`: its building fires its power.
    SpecialPower,
    /// Any other, which the import leaves.
    #[default]
    Other,
}

impl CommandButtonIni {
    /// Reads one INI file, adding its buttons to those read before: a button read again keeps
    /// the fields it does not name and takes the new ones, as an override does.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        for block in IniText::of(bytes)
            .lines()
            .blocks(BlockKinds::One("commandbutton"))
        {
            let block = block?;
            let button = self.buttons.entry(block.name()?.to_owned()).or_default();
            for line in &block.fields {
                match line.word() {
                    "command" => {
                        button.command = match line.value()?.to_ascii_uppercase().as_str() {
                            "UNIT_BUILD" => ButtonCommand::UnitBuild,
                            "DOZER_CONSTRUCT" => ButtonCommand::DozerConstruct,
                            "PLAYER_UPGRADE" => ButtonCommand::PlayerUpgrade,
                            "OBJECT_UPGRADE" => ButtonCommand::ObjectUpgrade,
                            "SELL" => ButtonCommand::Sell,
                            "STOP" => ButtonCommand::Stop,
                            "ATTACK_MOVE" => ButtonCommand::AttackMove,
                            "SET_RALLY_POINT" => ButtonCommand::SetRallyPoint,
                            "PURCHASE_SCIENCE" => ButtonCommand::PurchaseScience,
                            "SPECIAL_POWER" => ButtonCommand::SpecialPower,
                            _ => ButtonCommand::Other,
                        };
                    }
                    "object" => button.object = Some(line.value()?.to_owned()),
                    "upgrade" => button.upgrade = Some(line.value()?.to_owned()),
                    // `parseScienceVector` starts the list again, and a `NONE` anywhere in it
                    // empties it.
                    "science" => {
                        let sciences = &line.tokens[1..];
                        let none = sciences
                            .iter()
                            .any(|token| token.eq_ignore_ascii_case("none"));
                        button.sciences.clear();
                        if !none {
                            button
                                .sciences
                                .extend(sciences.iter().map(|&token| token.to_owned()));
                        }
                    }
                    "specialpower" => button.power = Some(line.value()?.to_owned()),
                    "textlabel" => button.label = Some(line.value()?.to_owned()),
                    "buttonimage" => button.image = Some(line.value()?.to_owned()),
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// The button `name`; none for a name no file defines.
    pub(crate) fn get(&self, name: &str) -> Option<&CommandButton> {
        self.buttons.get(name)
    }
}

#[cfg(test)]
pub(crate) mod samples {
    /// `Data\INI\CommandButton.ini` of the fixture install: the buttons of the fixture's command
    /// sets.
    pub(crate) const COMMAND_BUTTONS: &str = "CommandButton Command_ConstructAmericaDozer\r
  Command = UNIT_BUILD\r
  Object = AmericaVehicleDozer\r
  TextLabel = CONTROLBAR:ConstructAmericaDozer\r
  ButtonImage = SACDozer\r
End\r
CommandButton Command_SetRallyPoint\r
  Command = SET_RALLY_POINT\r
  TextLabel = CONTROLBAR:SetRallyPoint\r
End\r
CommandButton Command_ConstructAmericaBarracks\r
  Command = DOZER_CONSTRUCT\r
  Object = AmericaBarracks\r
  TextLabel = CONTROLBAR:ConstructAmericaBarracks\r
End\r
CommandButton Command_ConstructAmericaPowerPlant\r
  Command = DOZER_CONSTRUCT\r
  Object = AmericaPowerPlant\r
  TextLabel = CONTROLBAR:ConstructAmericaPowerPlant\r
End\r
CommandButton Command_ConstructAmericaFireBase\r
  Command = DOZER_CONSTRUCT\r
  Object = AmericaFireBase\r
  TextLabel = CONTROLBAR:ConstructAmericaFireBase\r
End\r
CommandButton Command_ConstructAmericaInfantryRanger\r
  Command = UNIT_BUILD\r
  Object = AmericaInfantryRanger\r
  TextLabel = CONTROLBAR:ConstructAmericaInfantryRanger\r
End\r
CommandButton Command_ConstructAmericaInfantryPathfinder\r
  Command = UNIT_BUILD\r
  Object = AmericaInfantryPathfinder\r
  TextLabel = CONTROLBAR:ConstructAmericaInfantryPathfinder\r
End\r
CommandButton Command_ConstructFortressWall\r
  Command = UNIT_BUILD\r
  Object = FortressWall\r
  TextLabel = CONTROLBAR:ConstructFortressWall\r
End\r
CommandButton Command_ConstructGhost\r
  Command = UNIT_BUILD\r
  Object = Lost\r
End\r
CommandButton Command_UpgradeAmericaCompositeArmor\r
  Command = PLAYER_UPGRADE\r
  Upgrade = Upgrade_AmericaCompositeArmor\r
  TextLabel = CONTROLBAR:UpgradeCompositeArmor\r
End\r
CommandButton Command_UpgradeAmericaRangerFlashBang\r
  Command = PLAYER_UPGRADE\r
  Upgrade = Upgrade_AmericaRangerFlashBang\r
End\r
CommandButton Command_UpgradeAmericaBarracksWalls\r
  Command = OBJECT_UPGRADE\r
  Upgrade = Upgrade_AmericaBarracksWalls\r
  Science = None\r
End\r
CommandButton Command_UpgradeAmericaLaserMissiles\r
  Command = PLAYER_UPGRADE\r
  Upgrade = Upgrade_AmericaCompositeArmor\r
  Science = SCIENCE_AMERICA\r
End\r
CommandButton Command_PurchaseScienceParadrop1\r
  Command = PURCHASE_SCIENCE\r
  Science = SCIENCE_Paradrop1\r
  ButtonImage = SAParadrop\r
End\r
CommandButton Command_PurchaseScienceParadrop2\r
  Command = PURCHASE_SCIENCE\r
  Science = SCIENCE_Paradrop2\r
  ButtonImage = SAParadrop\r
End\r
CommandButton Command_UpgradeLost\r
  Command = OBJECT_UPGRADE\r
  Upgrade = Upgrade_Lost\r
End\r
";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::format::ini::command_button_ini::samples::COMMAND_BUTTONS;

    #[test]
    fn a_button_takes_its_command_object_and_label_a_later_file_over_the_fields_it_names() {
        let mut ini = CommandButtonIni::default();
        ini.read(COMMAND_BUTTONS.as_bytes()).unwrap();
        let dozer = CommandButton {
            command: ButtonCommand::UnitBuild,
            object: Some("AmericaVehicleDozer".to_owned()),
            upgrade: None,
            label: Some("CONTROLBAR:ConstructAmericaDozer".to_owned()),
            image: Some("SACDozer".to_owned()),
            sciences: Vec::new(),
            power: None,
        };
        assert_eq!(ini.get("Command_ConstructAmericaDozer"), Some(&dozer));
        let rally = ini.get("Command_SetRallyPoint").unwrap();
        assert_eq!(
            (rally.command, &rally.object),
            (ButtonCommand::SetRallyPoint, &None)
        );
        assert_eq!(
            ini.get("Command_ConstructAmericaBarracks").unwrap().command,
            ButtonCommand::DozerConstruct
        );
        assert!(ini.get("command_constructamericadozer").is_none());
        // An upgrade's button names it; a science of `NONE` is none.
        let armor = ini.get("Command_UpgradeAmericaCompositeArmor").unwrap();
        assert_eq!(
            (
                armor.command,
                armor.upgrade.as_deref(),
                armor.sciences.is_empty()
            ),
            (
                ButtonCommand::PlayerUpgrade,
                Some("Upgrade_AmericaCompositeArmor"),
                true
            )
        );
        let walls = ini.get("Command_UpgradeAmericaBarracksWalls").unwrap();
        assert_eq!(
            (walls.command, walls.sciences.is_empty()),
            (ButtonCommand::ObjectUpgrade, true)
        );
        let laser = ini.get("Command_UpgradeAmericaLaserMissiles").unwrap();
        assert_eq!(laser.sciences, ["SCIENCE_AMERICA"]);
        // A purchase names the science it buys.
        let paradrop = ini.get("Command_PurchaseScienceParadrop2").unwrap();
        assert_eq!(paradrop.command, ButtonCommand::PurchaseScience);
        assert_eq!(paradrop.sciences, ["SCIENCE_Paradrop2"]);
        // A later file keeps the fields it does not name; a command of either case is read.
        ini.read(b"CommandButton Command_ConstructAmericaDozer\n Command = dozer_construct\nEnd\n")
            .unwrap();
        let again = ini.get("Command_ConstructAmericaDozer").unwrap();
        assert_eq!(
            again,
            &CommandButton {
                command: ButtonCommand::DozerConstruct,
                ..dozer
            }
        );
        let fails = |text: &[u8]| CommandButtonIni::default().read(text).unwrap_err();
        assert_eq!(
            fails(b"CommandButton A\n Object\nEnd\n"),
            IniError::NoValue { line: 2 }
        );
        assert_eq!(
            fails(b"Command = UNIT_BUILD\n"),
            IniError::NotBlock { line: 1 }
        );
    }
}
