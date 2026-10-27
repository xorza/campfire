use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::IniLine;

/// An object's upgrade module, as `UpgradeMuxData` and its module's data read it: what it does, the
/// upgrades any one of which, or with `all` every one, triggers it, `TriggeredBy` and
/// `RequiresAllTriggers`, and those that keep it from triggering, `ConflictsWith`, each list as
/// its last line gives it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IniUpgradeModule {
    pub(crate) effect: UpgradeEffect,
    pub(crate) triggers: Vec<String>,
    pub(crate) conflicts: Vec<String>,
    pub(crate) all: bool,
}

/// What an upgrade module does.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum UpgradeEffect {
    /// `ArmorUpgrade`: its object takes its armor set of `PLAYER_UPGRADE`.
    Armor,
    /// `WeaponSetUpgrade`: its object takes its weapon set of `PLAYER_UPGRADE`.
    WeaponSet,
    /// `WeaponBonusUpgrade`: its object's weapons take their bonuses of `PLAYER_UPGRADE`.
    WeaponBonus,
    /// `MaxHealthUpgrade`: its object's most life rises by `add`, its life changing by `change`.
    MaxHealth { add: f32, change: HealthChange },
    /// `PowerPlantUpgrade`: its player's power rises and its object's rods extend, as its
    /// `PowerPlantUpdate` sets `POWER_PLANT_UPGRADED`.
    PowerPlant,
    /// `RadarUpgrade`: its object gives its player a radar (`Player::hasRadar`).
    Radar,
    /// `ModelConditionUpgrade`: its object holds the model condition `flag`, its
    /// `ConditionFlag`, none for none named.
    ModelCondition { flag: Option<String> },
    /// `SubObjectsUpgrade`: its object's drawable shows the sub-objects `show` and hides `hide`,
    /// its `ShowSubObjects` and `HideSubObjects`, in every state.
    SubObjects {
        show: Vec<String>,
        hide: Vec<String>,
    },
    /// Any other upgrade module, by its class's name as the object writes it.
    Other(String),
}

/// How a `MaxHealthUpgrade` changes its object's life, of `TheMaxHealthChangeTypeNames`,
/// `SAME_CURRENTHEALTH` by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HealthChange {
    SameCurrent,
    PreserveRatio,
    AddCurrentToo,
    FullyHeal,
}

impl IniUpgradeModule {
    /// The module a `Behavior` line of the class `class`, of either case, opens, as it is as the
    /// game makes it, of no trigger; none for a class that is no upgrade module, as every one's
    /// name ends in `Upgrade`.
    pub(crate) fn module(class: &str) -> Option<Self> {
        let effect = match class.to_ascii_lowercase().as_str() {
            "armorupgrade" => UpgradeEffect::Armor,
            "weaponsetupgrade" => UpgradeEffect::WeaponSet,
            "weaponbonusupgrade" => UpgradeEffect::WeaponBonus,
            "maxhealthupgrade" => UpgradeEffect::MaxHealth {
                add: 0.0,
                change: HealthChange::SameCurrent,
            },
            "powerplantupgrade" => UpgradeEffect::PowerPlant,
            "radarupgrade" => UpgradeEffect::Radar,
            "modelconditionupgrade" => UpgradeEffect::ModelCondition { flag: None },
            "subobjectsupgrade" => UpgradeEffect::SubObjects {
                show: Vec::new(),
                hide: Vec::new(),
            },
            lower if lower.ends_with("upgrade") => UpgradeEffect::Other(class.to_owned()),
            _ => return None,
        };
        Some(Self {
            effect,
            triggers: Vec::new(),
            conflicts: Vec::new(),
            all: false,
        })
    }

    /// The name of its module's class, as the game names it.
    pub(crate) fn class(&self) -> &str {
        match &self.effect {
            UpgradeEffect::Armor => "ArmorUpgrade",
            UpgradeEffect::WeaponSet => "WeaponSetUpgrade",
            UpgradeEffect::WeaponBonus => "WeaponBonusUpgrade",
            UpgradeEffect::MaxHealth { .. } => "MaxHealthUpgrade",
            UpgradeEffect::PowerPlant => "PowerPlantUpgrade",
            UpgradeEffect::Radar => "RadarUpgrade",
            UpgradeEffect::ModelCondition { .. } => "ModelConditionUpgrade",
            UpgradeEffect::SubObjects { .. } => "SubObjectsUpgrade",
            UpgradeEffect::Other(class) => class,
        }
    }

    /// Takes the module's line of `word`, sub-objects appended as `parseAsciiStringVectorAppend`
    /// appends them; a change type of no name the game lists fails the read, as
    /// `INI::parseIndexList` does.
    pub(crate) fn field(&mut self, word: &str, at: &IniLine<'_>) -> Result<(), IniError> {
        let names =
            || -> Vec<String> { at.tokens[1..].iter().map(|&name| name.to_owned()).collect() };
        match (word, &mut self.effect) {
            ("triggeredby", _) => self.triggers = names(),
            ("conflictswith", _) => self.conflicts = names(),
            ("requiresalltriggers", _) => self.all = at.yes()?,
            ("addmaxhealth", UpgradeEffect::MaxHealth { add, .. }) => *add = at.real()?,
            ("conditionflag", UpgradeEffect::ModelCondition { flag }) => {
                *flag = Some(at.value()?.to_ascii_lowercase());
            }
            ("showsubobjects", UpgradeEffect::SubObjects { show, .. }) => show.extend(names()),
            ("hidesubobjects", UpgradeEffect::SubObjects { hide, .. }) => hide.extend(names()),
            ("changetype", UpgradeEffect::MaxHealth { change, .. }) => {
                *change = match at.value()?.to_ascii_uppercase().as_str() {
                    "SAME_CURRENTHEALTH" => HealthChange::SameCurrent,
                    "PRESERVE_RATIO" => HealthChange::PreserveRatio,
                    "ADD_CURRENT_HEALTH_TOO" => HealthChange::AddCurrentToo,
                    "FULLY_HEAL" => HealthChange::FullyHeal,
                    _ => return Err(IniError::NotListed { line: at.number }),
                };
            }
            _ => {}
        }
        Ok(())
    }
}
