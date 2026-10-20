use campfire_mode::OptionData;

use crate::zero_hour::format::ini::error::{IniError, MoneyError};
use crate::zero_hour::format::ini::ini_text::{BlockKinds, IniLine, IniText};

/// What the import reads of Zero Hour's `Multiplayer.ini` files: the choices of starting money,
/// as `parseMultiplayerStartingMoneyChoiceDefinition` adds them, each with whether it is the
/// default a skirmish starts with; and the players' colors, each `MultiplayerColor`'s `RGBColor`
/// as `parseMultiplayerColorDefinition` adds them, in the files' order, as its search by name
/// matches a tooltip's name and finds none.
#[derive(Debug, Default)]
pub(crate) struct MultiplayerIni {
    money: Vec<MoneyChoice>,
    colors: Vec<[u8; 3]>,
}

/// The starting money a skirmish's host picks among, in the order the files give them, and the
/// one a skirmish of no pick starts with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MoneyChoices {
    pub(crate) values: Vec<u32>,
    pub(crate) default: u32,
}

/// A `MultiplayerStartingMoneyChoice`: its `Value`, and its `Default`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct MoneyChoice {
    value: u32,
    default: bool,
}

impl MultiplayerIni {
    /// Reads one INI file, adding its choices of starting money and its colors to those read
    /// before; its other blocks, each of no inner block, it passes over.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        for block in IniText::of(bytes).blocks(BlockKinds::Any) {
            let block = block?;
            match block.head.word().as_str() {
                "multiplayerstartingmoneychoice" => {}
                "multiplayercolor" => {
                    self.colors.push(Self::color(&block.fields)?);
                    continue;
                }
                _ => continue,
            }
            let mut choice = MoneyChoice::default();
            for line in &block.fields {
                match line.word().as_str() {
                    "value" => {
                        let value = line.int()?;
                        choice.value = u32::try_from(value)
                            .map_err(|_| IniError::Range { line: line.number })?;
                    }
                    "default" => choice.default = line.yes()?,
                    _ => {}
                }
            }
            self.money.push(choice);
        }
        Ok(())
    }

    /// The players' colors, in the files' order.
    pub(crate) fn colors(&self) -> &[[u8; 3]] {
        &self.colors
    }

    /// A color's `RGBColor`, as `INI::parseRGBColor` reads it: each channel a whole number from
    /// 0 to 255; black for a color of none, as the game's definition starts.
    fn color(fields: &[IniLine<'_>]) -> Result<[u8; 3], IniError> {
        let mut color = [0; 3];
        for line in fields {
            if line.word() != "rgbcolor" {
                continue;
            }
            let channels = line.int_coord(["R", "G", "B"])?;
            for (channel, value) in color.iter_mut().zip(channels) {
                *channel =
                    u8::try_from(value).map_err(|_| IniError::Range { line: line.number })?;
            }
        }
        Ok(color)
    }

    /// The choices of starting money, the default the one marked `Default`; an error unless
    /// exactly one is, as the game asserts, for a value of two choices, which no host could tell
    /// apart, and for more than a mode's option holds.
    pub(crate) fn money(&self) -> Result<MoneyChoices, MoneyError> {
        let mut defaults = self.money.iter().filter(|choice| choice.default);
        let default = match (defaults.next(), defaults.next()) {
            (Some(choice), None) => choice.value,
            _ => return Err(MoneyError::Default),
        };
        let values: Vec<u32> = self.money.iter().map(|choice| choice.value).collect();
        if let Some(at) = (1..values.len()).find(|&at| values[..at].contains(&values[at])) {
            return Err(MoneyError::Twice(values[at]));
        }
        if values.len() > OptionData::MAX_VALUES {
            return Err(MoneyError::TooMany(values.len()));
        }
        Ok(MoneyChoices { values, default })
    }
}

#[cfg(test)]
pub(crate) mod samples {
    /// `Data\INI\Multiplayer.ini` of the fixture install: settings, which the import passes over,
    /// two colors, and three choices of starting money, the second the default.
    pub(crate) const MULTIPLAYER: &str = "MultiplayerSettings\r
  StartCountdownTimer = 5\r
End\r
MultiplayerColor ColorGold\r
  RGBColor = R:255 G:200 B:0\r
  TooltipName = Color:Gold\r
End\r
MultiplayerColor ColorRed\r
  RGBColor = R:255 G:0 B:0\r
End\r
MultiplayerStartingMoneyChoice\r
  Value = 5000\r
End\r
MultiplayerStartingMoneyChoice\r
  Value = 10000\r
  Default = Yes\r
End\r
MultiplayerStartingMoneyChoice\r
  Value = 20000\r
  Default = No\r
End\r
";
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::format::ini::multiplayer_ini::samples::MULTIPLAYER;

    #[test]
    fn a_skirmish_starts_with_the_one_choice_marked_default() {
        let mut ini = MultiplayerIni::default();
        ini.read(MULTIPLAYER.as_bytes()).unwrap();
        // The colors in their order, a channel of a fraction its whole part, as `scanInt` reads
        // it; a channel past 255 or below 0 is refused.
        assert_eq!(ini.colors(), [[255, 200, 0], [255, 0, 0]]);
        let mut whole = MultiplayerIni::default();
        whole
            .read(b"MultiplayerColor Half\n RGBColor = R:1.5 G:2 B:3\nEnd\n")
            .unwrap();
        assert_eq!(whole.colors(), [[1, 2, 3]]);
        for wrong in ["R:256 G:0 B:0", "R:-1 G:0 B:0"] {
            let text = format!("MultiplayerColor Wrong\n RGBColor = {wrong}\nEnd\n");
            let read = MultiplayerIni::default().read(text.as_bytes());
            assert!(matches!(read, Err(IniError::Range { line: 2 })), "{wrong}");
        }
        assert_eq!(
            ini.money(),
            Ok(MoneyChoices {
                values: vec![5000, 10000, 20000],
                default: 10000,
            })
        );
        // A second default, in a later file, leaves none the game takes; and so does a file of
        // no default. A value of two choices is refused, as is a list past 256.
        let mut twice = MultiplayerIni::default();
        twice.read(MULTIPLAYER.as_bytes()).unwrap();
        twice
            .read(b"MultiplayerStartingMoneyChoice\n Value = 5000\nEnd\n")
            .unwrap();
        assert_eq!(twice.money(), Err(MoneyError::Twice(5000)));
        ini.read(b"MultiplayerStartingMoneyChoice\n Value = 1\n Default = Yes\nEnd\n")
            .unwrap();
        assert_eq!(ini.money(), Err(MoneyError::Default));
        let mut none = MultiplayerIni::default();
        none.read(b"MultiplayerStartingMoneyChoice\n Value = 1\nEnd\n")
            .unwrap();
        assert_eq!(none.money(), Err(MoneyError::Default));
        let mut many = MultiplayerIni::default();
        for value in 0..=256 {
            let default = if value == 0 { "Yes" } else { "No" };
            let text = format!(
                "MultiplayerStartingMoneyChoice\n Value = {value}\n Default = {default}\nEnd\n"
            );
            many.read(text.as_bytes()).unwrap();
        }
        assert_eq!(many.money(), Err(MoneyError::TooMany(257)));
        let fails = |text: &[u8]| MultiplayerIni::default().read(text).unwrap_err();
        assert_eq!(
            fails(b"MultiplayerStartingMoneyChoice\n Value = -1\nEnd\n"),
            IniError::Range { line: 2 }
        );
        assert_eq!(fails(b"End\n"), IniError::StrayEnd { line: 1 });
        assert_eq!(fails(b"MultiplayerSettings\n"), IniError::Unclosed);
    }
}
