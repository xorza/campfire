use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::IniLine;

/// An object's special powers and heal burst, as their modules read them: each `OCLSpecialPower`
/// and its `AutoHealBehavior`, the last one read.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct IniPowers {
    pub(crate) ocl: Vec<IniOclPower>,
    pub(crate) heal: Option<IniAutoHeal>,
}

/// An `OCLSpecialPower`: its power's template, `SpecialPowerTemplate`, the creation list it makes,
/// `OCL`, the list each science upgrades it to, `UpgradeOCL`, in their order, and whether it makes
/// the list where its player aims, `CreateLocation = CREATE_AT_LOCATION`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct IniOclPower {
    pub(crate) power: Option<String>,
    pub(crate) list: Option<String>,
    pub(crate) upgrades: Vec<UpgradeList>,
    pub(crate) at_location: bool,
}

/// An `UpgradeOCL` line: the science and the creation list it makes in place of the power's own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UpgradeList {
    pub(crate) science: String,
    pub(crate) list: String,
}

/// An `AutoHealBehavior`: what it heals, `HealingAmount`, within `Radius`, the objects of its
/// `KindOf`, by name, and whether it heals once, `SingleBurst`, and starts on, `StartsActive`.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct IniAutoHeal {
    pub(crate) amount: i32,
    pub(crate) radius: f32,
    pub(crate) kinds: Vec<String>,
    pub(crate) single_burst: bool,
    pub(crate) starts_active: bool,
}

/// A module of `IniPowers` being read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PowerModule {
    Ocl,
    Heal,
}

impl IniPowers {
    /// Opens the behavior module `module`, of the name its line gives in lowercase, when it is one
    /// it reads.
    pub(crate) fn module(&mut self, module: &str) -> Option<PowerModule> {
        match module {
            "oclspecialpower" => {
                self.ocl.push(IniOclPower::default());
                Some(PowerModule::Ocl)
            }
            "autohealbehavior" => {
                self.heal = Some(IniAutoHeal::default());
                Some(PowerModule::Heal)
            }
            _ => None,
        }
    }

    /// Takes the line of `word` of the open `module`, a field it reads or another.
    pub(crate) fn module_field(
        &mut self,
        module: PowerModule,
        word: &str,
        at: &IniLine<'_>,
    ) -> Result<(), IniError> {
        match module {
            PowerModule::Ocl => {
                let power = self.ocl.last_mut().expect("a power's module is open");
                match word {
                    "specialpowertemplate" => power.power = Some(at.value()?.to_owned()),
                    "ocl" => power.list = Some(at.value()?.to_owned()),
                    "upgradeocl" => {
                        let list = at
                            .tokens
                            .get(2)
                            .ok_or(IniError::NoValue { line: at.number })?;
                        power.upgrades.push(UpgradeList {
                            science: at.value()?.to_owned(),
                            list: (*list).to_owned(),
                        });
                    }
                    "createlocation" => {
                        power.at_location = at.value()?.eq_ignore_ascii_case("create_at_location");
                    }
                    _ => {}
                }
            }
            PowerModule::Heal => {
                let heal = self.heal.as_mut().expect("a heal's module is open");
                match word {
                    "healingamount" => heal.amount = at.int()?,
                    "radius" => heal.radius = at.real()?,
                    "kindof" => {
                        heal.kinds = at.tokens[1..].iter().map(|&kind| kind.to_owned()).collect();
                    }
                    "singleburst" => heal.single_burst = at.yes()?,
                    "startsactive" => heal.starts_active = at.yes()?,
                    _ => {}
                }
            }
        }
        Ok(())
    }
}
