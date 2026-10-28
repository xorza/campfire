use crate::zero_hour::format::ini::error::IniError;
use crate::zero_hour::format::ini::ini_text::IniLine;
use crate::zero_hour::format::ini::object_ini::object_kinds::{KindMask, ObjectKind, ObjectKinds};

/// An object's contain module, as `OpenContainModuleData` and the module's own data read it: its
/// class; its `Slots` of a transport, its `ContainMax` of another; the kinds it takes,
/// `AllowInsideKindOf`, infantry by default for a transport and a garrison, every kind for
/// another, and those it refuses, `ForbidInsideKindOf`; whether it takes its allies, its enemies
/// and the neutral, each `Yes` by default; whether its passengers fire, `PassengersAllowedToFire`;
/// the share of each rider's most health its death deals it, `DamagePercentToUnits`; a transport's
/// milliseconds between two exits, `ExitDelay`, the riders it is made with, `InitialPayload`, and
/// the share of its riders' most health it heals each second, `HealthRegen%PerSec`; and whether a
/// garrison encloses its riders, `IsEnclosingContainer`, yes by default.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct IniContain {
    pub(crate) class: ContainClass,
    pub(crate) capacity: i32,
    pub(crate) allow: KindMask,
    pub(crate) forbid: KindMask,
    pub(crate) allies: bool,
    pub(crate) enemies: bool,
    pub(crate) neutral: bool,
    pub(crate) fire: bool,
    pub(crate) damage: f32,
    pub(crate) exit_ms: i32,
    pub(crate) payload: Option<IniPayload>,
    pub(crate) heal: f32,
    pub(crate) enclosing: bool,
}

/// The class of a contain module, as `ModuleFactory` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContainClass {
    Transport,
    Garrison,
    Open,
    Cave,
    Heal,
    InternetHack,
    RailedTransport,
    RiderChange,
    MobNexus,
    Tunnel,
    Overlord,
    Helix,
    Parachute,
}

/// The riders a transport is made with: `count` of the object `name`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IniPayload {
    pub(crate) name: String,
    pub(crate) count: i32,
}

impl ContainClass {
    /// Each class, by its name in lowercase.
    const NAMED: [(&str, Self); 13] = [
        ("transportcontain", Self::Transport),
        ("garrisoncontain", Self::Garrison),
        ("opencontain", Self::Open),
        ("cavecontain", Self::Cave),
        ("healcontain", Self::Heal),
        ("internethackcontain", Self::InternetHack),
        ("railedtransportcontain", Self::RailedTransport),
        ("riderchangecontain", Self::RiderChange),
        ("mobnexuscontain", Self::MobNexus),
        ("tunnelcontain", Self::Tunnel),
        ("overlordcontain", Self::Overlord),
        ("helixcontain", Self::Helix),
        ("parachutecontain", Self::Parachute),
    ];
}

impl IniContain {
    /// The module a `Behavior` line of the class `class`, of either case, opens, as the game makes
    /// it; none for a class of no contain module.
    pub(crate) fn module(class: &str) -> Option<Self> {
        let class = class.to_ascii_lowercase();
        let (_, class) = ContainClass::NAMED
            .into_iter()
            .find(|&(name, _)| name == class)?;
        let allow = match class {
            ContainClass::Transport | ContainClass::Garrison => {
                KindMask::of(ObjectKinds::of(&[ObjectKind::Infantry]))
            }
            _ => KindMask {
                kinds: ObjectKinds::default(),
                unread: true,
            },
        };
        Some(Self {
            class,
            capacity: 0,
            allow,
            forbid: KindMask::default(),
            allies: true,
            enemies: true,
            neutral: true,
            fire: false,
            damage: 0.0,
            exit_ms: 0,
            payload: None,
            heal: 0.0,
            enclosing: true,
        })
    }

    /// Takes the module's line of `word`, a field it reads or another.
    pub(crate) fn field(&mut self, word: &str, at: &IniLine<'_>) -> Result<(), IniError> {
        // A Helix's and an Overlord's contain are transports of their own (`HelixContain` and
        // `OverlordContain` derive from `TransportContain`).
        let transport = matches!(
            self.class,
            ContainClass::Transport | ContainClass::Helix | ContainClass::Overlord
        );
        match word {
            "slots" if transport => self.capacity = at.int()?,
            "containmax" if !transport => self.capacity = at.int()?,
            "allowinsidekindof" => self.allow.set_by(at)?,
            "forbidinsidekindof" => self.forbid.set_by(at)?,
            "allowalliesinside" => self.allies = at.yes()?,
            "allowenemiesinside" => self.enemies = at.yes()?,
            "allowneutralinside" => self.neutral = at.yes()?,
            "passengersallowedtofire" => self.fire = at.yes()?,
            "damagepercenttounits" => self.damage = at.real()? / 100.0,
            "exitdelay" if transport => self.exit_ms = at.int()?,
            "healthregen%persec" if transport => self.heal = at.real()?,
            "initialpayload" if transport => {
                self.payload = Some(IniPayload {
                    name: at.value()?.to_owned(),
                    count: if at.tokens.len() > 2 {
                        at.int_at(2)?
                    } else {
                        1
                    },
                });
            }
            "isenclosingcontainer" if self.class == ContainClass::Garrison => {
                self.enclosing = at.yes()?;
            }
            _ => {}
        }
        Ok(())
    }
}
