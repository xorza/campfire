use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::IniLine;
use crate::zero_hour::format::ini::object_ini::SLOTS;

/// What an object gives combat, as `ThingTemplate` and its body module read it: its body, none
/// with none, and its weapon sets and armor sets in the order it gives them, each appended as
/// `parseWeaponTemplateSet` and `parseArmorTemplateSet` append them. A reskin keeps its base's,
/// as its table sets none of them.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct FightData {
    pub(crate) body: Option<IniBody>,
    pub(crate) weapon_sets: Vec<WeaponSet>,
    pub(crate) armor_sets: Vec<ArmorSet>,
}

/// An object's `Body` module: its kind, its `MaxHealth` and its `InitialHealth`, 0 when absent,
/// as `ActiveBodyModuleData` starts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct IniBody {
    pub(crate) kind: BodyKind,
    pub(crate) max: f32,
    pub(crate) initial: f32,
}

/// A body module, of those the import reads as life: `ActiveBody` and `StructureBody`; the others,
/// `ImmortalBody`, `HighlanderBody`, `UndeadBody`, `HiveStructureBody` and `InactiveBody`, keep a
/// unit alive or move its damage by rules of their own (12-zero-hour.md, Life is the original's).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BodyKind {
    Active,
    Structure,
    Other,
}

/// A `WeaponSet`: how many conditions it is for, and its weapons by slot, `PRIMARY`,
/// `SECONDARY` then `TERTIARY`, none for a slot it leaves empty or names `NONE`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct WeaponSet {
    pub(crate) conditions: usize,
    pub(crate) weapons: [Option<String>; 3],
}

/// An `ArmorSet`: how many conditions it is for, and its armor, none for `NONE` or none named.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ArmorSet {
    pub(crate) conditions: usize,
    pub(crate) armor: Option<String>,
}

/// A block of combat an object's reader is inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FightBlock {
    Body,
    WeaponSet,
    ArmorSet,
}

impl FightData {
    /// The block a line of an object's own block of `word` and `tokens` opens, of those combat
    /// reads, once it began it; none for another line. `Body` opens with its module's name,
    /// `WeaponSet` and `ArmorSet` alone on their line.
    pub(crate) fn open(&mut self, word: &str, tokens: &[&str]) -> Option<FightBlock> {
        match word {
            "body" => {
                let kind = match tokens
                    .get(1)
                    .map(|kind| kind.to_ascii_lowercase())
                    .as_deref()
                {
                    Some("activebody") => BodyKind::Active,
                    Some("structurebody") => BodyKind::Structure,
                    _ => BodyKind::Other,
                };
                self.body = Some(IniBody {
                    kind,
                    max: 0.0,
                    initial: 0.0,
                });
                Some(FightBlock::Body)
            }
            "weaponset" if tokens.len() == 1 => {
                self.weapon_sets.push(WeaponSet::default());
                Some(FightBlock::WeaponSet)
            }
            "armorset" if tokens.len() == 1 => {
                self.armor_sets.push(ArmorSet::default());
                Some(FightBlock::ArmorSet)
            }
            _ => None,
        }
    }

    /// Takes the line of `word` inside the block `block`, the last it opened.
    pub(crate) fn field(
        &mut self,
        block: FightBlock,
        word: &str,
        at: &IniLine<'_>,
    ) -> Result<(), IniError> {
        let named = |value: &str| (!value.eq_ignore_ascii_case("none")).then(|| value.to_owned());
        let conditions = || {
            at.tokens[1..]
                .iter()
                .filter(|flag| !flag.eq_ignore_ascii_case("none"))
                .count()
        };
        match (block, word) {
            (FightBlock::Body, "maxhealth") => self.body_mut().max = at.real()?,
            (FightBlock::Body, "initialhealth") => self.body_mut().initial = at.real()?,
            (FightBlock::WeaponSet, "conditions") => self.last_weapons().conditions = conditions(),
            (FightBlock::WeaponSet, "weapon") => {
                let slot = at.value()?.to_ascii_lowercase();
                let slot = SLOTS
                    .iter()
                    .position(|known| *known == slot)
                    .ok_or(IniError::WeaponSlot { line: at.number })?;
                let name = at
                    .tokens
                    .get(2)
                    .ok_or(IniError::NoValue { line: at.number })?;
                self.last_weapons().weapons[slot] = named(name);
            }
            (FightBlock::ArmorSet, "conditions") => self.last_armor().conditions = conditions(),
            (FightBlock::ArmorSet, "armor") => self.last_armor().armor = named(at.value()?),
            _ => {}
        }
        Ok(())
    }

    /// The weapon set it starts with, of no condition set: the one of the fewest conditions,
    /// the first on a tie, as the game's sparse match finds it for no flag; none with none.
    pub(crate) fn starting_weapons(&self) -> Option<&WeaponSet> {
        self.weapon_sets.iter().min_by_key(|set| set.conditions)
    }

    /// The armor it starts with, found as its weapon set is; none with no set or none named.
    pub(crate) fn starting_armor(&self) -> Option<&str> {
        let set = self.armor_sets.iter().min_by_key(|set| set.conditions)?;
        set.armor.as_deref()
    }

    const fn body_mut(&mut self) -> &mut IniBody {
        self.body.as_mut().expect("a body block opened its body")
    }

    fn last_weapons(&mut self) -> &mut WeaponSet {
        self.weapon_sets
            .last_mut()
            .expect("a weapon set block opened its set")
    }

    fn last_armor(&mut self) -> &mut ArmorSet {
        self.armor_sets
            .last_mut()
            .expect("an armor set block opened its set")
    }
}
