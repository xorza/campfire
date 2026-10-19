use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::IniLine;
use crate::zero_hour::format::ini::object_ini::SLOTS;

/// What an object gives combat, as `ThingTemplate` and its modules read it: its body, none with
/// none, its weapon sets and armor sets in the order it gives them, each appended as
/// `parseWeaponTemplateSet` and `parseArmorTemplateSet` append them, its shell's flight, of a
/// `DumbProjectileBehavior`, and the first turret of its AI module. A reskin keeps its base's,
/// as its table sets none of them.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct FightData {
    pub(crate) body: Option<IniBody>,
    pub(crate) weapon_sets: Vec<WeaponSet>,
    pub(crate) armor_sets: Vec<ArmorSet>,
    pub(crate) shell: Option<IniShell>,
    pub(crate) turret: Option<IniTurret>,
}

/// A projectile object's `DumbProjectileBehavior`, as `DumbProjectileBehaviorModuleData` reads it,
/// each its default when absent: its curve's inner points, their heights above the way's highest
/// and their fractions of the way, how far its end follows its victim a second, and its life in
/// milliseconds, 10 seconds by default.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct IniShell {
    pub(crate) first_height: f32,
    pub(crate) second_height: f32,
    pub(crate) first_indent: f32,
    pub(crate) second_indent: f32,
    pub(crate) follow: f32,
    pub(crate) lifespan_ms: i32,
}

/// An AI module's `Turret`, as `TurretAIData` reads it: its `TurretTurnRate` in degrees a second,
/// 0.01 radians a frame by default, and the weapon slots it carries, `ControlledWeaponSlots`, none
/// by default.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct IniTurret {
    pub(crate) turn_rate: f32,
    pub(crate) slots: [bool; 3],
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
/// unit alive or move its damage by rules of their own (09-zero-hour.md, Life is the original's).
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
    Shell,
    /// An AI module, which may hold a turret.
    Ai,
    /// An AI module's first `Turret`.
    Turret,
}

impl IniShell {
    /// `DEFAULT_MAX_LIFESPAN`, 10 seconds.
    const LIFESPAN_MS: i32 = 10_000;

    pub(super) const DEFAULT: Self = Self {
        first_height: 0.0,
        second_height: 0.0,
        first_indent: 0.0,
        second_indent: 0.0,
        follow: 0.0,
        lifespan_ms: Self::LIFESPAN_MS,
    };
}

impl IniTurret {
    /// `DEFAULT_TURN_RATE`, 0.01 radians a frame at 30 frames a second, in degrees a second, each
    /// step in the game's `f32`.
    fn default_turn_rate() -> f32 {
        0.01 * 30.0 / (std::f32::consts::PI / 180.0)
    }
}

/// `percent`, as `parsePercentToReal` reads `50%`: a fraction of 1.
const fn percent(percent: f32) -> f32 {
    percent / 100.0
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
            "behavior" => {
                let module = tokens.get(1)?.to_ascii_lowercase();
                if module == "dumbprojectilebehavior" {
                    self.shell = Some(IniShell::DEFAULT);
                    Some(FightBlock::Shell)
                } else if module.ends_with("aiupdate") || module.ends_with("aiupdateinterface") {
                    Some(FightBlock::Ai)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Takes the line of `word` inside the block `block`, the last it opened; the block it opens
    /// inside it, of those combat reads: an AI module's first `Turret`, alone on its line.
    pub(crate) fn field(
        &mut self,
        block: FightBlock,
        word: &str,
        at: &IniLine<'_>,
    ) -> Result<Option<FightBlock>, IniError> {
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
            (FightBlock::Shell, field) => {
                let shell = self.shell.as_mut().expect("a shell block opened its shell");
                match field {
                    "firstheight" => shell.first_height = at.real()?,
                    "secondheight" => shell.second_height = at.real()?,
                    "firstpercentindent" => shell.first_indent = percent(at.real()?),
                    "secondpercentindent" => shell.second_indent = percent(at.real()?),
                    "flightpathadjustdistpersecond" => shell.follow = at.real()?,
                    "maxlifespan" => shell.lifespan_ms = at.int()?,
                    _ => {}
                }
            }
            // `parseTurret` reads the first turret; an `AltTurret` is a second, which the import
            // leaves.
            (FightBlock::Ai, "turret") if at.tokens.len() == 1 && self.turret.is_none() => {
                self.turret = Some(IniTurret {
                    turn_rate: IniTurret::default_turn_rate(),
                    slots: [false; 3],
                });
                return Ok(Some(FightBlock::Turret));
            }
            (FightBlock::Turret, "turretturnrate") => self.turret_mut().turn_rate = at.real()?,
            (FightBlock::Turret, "controlledweaponslots") => {
                let mut slots = [false; 3];
                for slot in &at.tokens[1..] {
                    let slot = slot.to_ascii_lowercase();
                    let at_slot = SLOTS.iter().position(|known| *known == slot);
                    let at_slot = at_slot.ok_or(IniError::WeaponSlot { line: at.number })?;
                    slots[at_slot] = true;
                }
                self.turret_mut().slots = slots;
            }
            _ => {}
        }
        Ok(None)
    }

    const fn turret_mut(&mut self) -> &mut IniTurret {
        self.turret
            .as_mut()
            .expect("a turret block opened its turret")
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
