use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::{BlockKinds, IniText};

/// What the import reads of Zero Hour's `Multiplayer.ini` files: the choices of starting money,
/// as `parseMultiplayerStartingMoneyChoiceDefinition` adds them, each with whether it is the
/// default a skirmish starts with.
#[derive(Debug, Default)]
pub(crate) struct MultiplayerIni {
    money: Vec<MoneyChoice>,
}

/// A `MultiplayerStartingMoneyChoice`: its `Value`, and its `Default`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct MoneyChoice {
    value: u32,
    default: bool,
}

impl MultiplayerIni {
    /// Reads one INI file, adding its choices of starting money to those read before; its other
    /// blocks, each of no inner block, it passes over.
    pub(crate) fn read(&mut self, bytes: &[u8]) -> Result<(), IniError> {
        for block in IniText::of(bytes).blocks(BlockKinds::Any) {
            let block = block?;
            if block.head.word() != "multiplayerstartingmoneychoice" {
                continue;
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

    /// The starting money of the one choice marked `Default`; `None` unless exactly one is, as
    /// the game asserts.
    pub(crate) fn default_money(&self) -> Option<u32> {
        let mut defaults = self.money.iter().filter(|choice| choice.default);
        match (defaults.next(), defaults.next()) {
            (Some(choice), None) => Some(choice.value),
            _ => None,
        }
    }
}

#[cfg(test)]
pub(crate) mod internals {
    /// `Data\INI\Multiplayer.ini` of the fixture install: settings and a color, which the import
    /// passes over, and three choices of starting money, the second the default.
    pub(crate) const MULTIPLAYER: &str = "MultiplayerSettings\r
  StartCountdownTimer = 5\r
End\r
MultiplayerColor ColorGold\r
  RGBColor = R:255 G:200 B:0\r
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
    use crate::zero_hour::format::ini::multiplayer_ini::internals::MULTIPLAYER;

    #[test]
    fn a_skirmish_starts_with_the_one_choice_marked_default() {
        let mut ini = MultiplayerIni::default();
        ini.read(MULTIPLAYER.as_bytes()).unwrap();
        assert_eq!(ini.default_money(), Some(10000));
        // A second default, in a later file, leaves none the game takes; and so does a file of
        // no default.
        ini.read(b"MultiplayerStartingMoneyChoice\n Value = 1\n Default = Yes\nEnd\n")
            .unwrap();
        assert_eq!(ini.default_money(), None);
        let mut none = MultiplayerIni::default();
        none.read(b"MultiplayerStartingMoneyChoice\n Value = 1\nEnd\n")
            .unwrap();
        assert_eq!(none.default_money(), None);
        let fails = |text: &[u8]| MultiplayerIni::default().read(text).unwrap_err();
        assert_eq!(
            fails(b"MultiplayerStartingMoneyChoice\n Value = -1\nEnd\n"),
            IniError::Range { line: 2 }
        );
        assert_eq!(fails(b"End\n"), IniError::StrayEnd { line: 1 });
        assert_eq!(fails(b"MultiplayerSettings\n"), IniError::Unclosed);
    }
}
