use campfire_common::EnumAll;
use campfire_values::DeclaredName;

/// Zero Hour's damage types, in the order of `DamageTypeFlags`' names: Generals' `FLESHY_SNIPER`
/// is not among them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub(crate) enum DamageType {
    Explosion,
    Crush,
    ArmorPiercing,
    SmallArms,
    Gattling,
    Radiation,
    Flame,
    Laser,
    Sniper,
    Poison,
    Healing,
    Unresistable,
    Water,
    Deploy,
    Surrender,
    Hack,
    KillPilot,
    Penalty,
    Falling,
    Melee,
    Disarm,
    HazardCleanup,
    ParticleBeam,
    Toppling,
    InfantryMissile,
    AuroraBomb,
    LandMine,
    JetMissiles,
    StealthjetMissiles,
    MolotovCocktail,
    ComancheVulcan,
    SubdualMissile,
    SubdualVehicle,
    SubdualBuilding,
    SubdualUnresistable,
    Microwave,
    KillGarrisoned,
    Status,
}

impl DamageType {
    /// The type `name`, of either case, as `INI::scanIndexList` finds it.
    pub(crate) fn of(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|known| name.eq_ignore_ascii_case(known.name()))
    }

    /// Its place in `ALL`.
    pub(crate) const fn index(self) -> usize {
        self as usize
    }

    /// Whether an armor's share applies to it: `ArmorTemplate::adjustDamage` passes
    /// `UNRESISTABLE` and `SUBDUAL_UNRESISTABLE` whole, whatever an armor says of them.
    pub(crate) const fn resisted(self) -> bool {
        !matches!(self, Self::Unresistable | Self::SubdualUnresistable)
    }

    /// Whether it takes life, as `IsHealthDamagingDamage` holds: a status, the four subdual
    /// types, which disable, and the kill of a vehicle's pilot or of a building's garrison take
    /// none. `HEALING` takes life here, as the function says, though `ActiveBody::attemptDamage`
    /// gives it to its target.
    pub(crate) const fn takes_life(self) -> bool {
        !matches!(
            self,
            Self::Status
                | Self::SubdualMissile
                | Self::SubdualVehicle
                | Self::SubdualBuilding
                | Self::SubdualUnresistable
                | Self::KillPilot
                | Self::KillGarrisoned
        )
    }

    /// The mode's damage kind of it, its name in lowercase.
    pub(crate) fn kind(self) -> DeclaredName {
        DeclaredName::new(&self.name().to_ascii_lowercase())
            .expect("a damage type is a declared name")
    }

    /// Its name, as the game's INI writes it.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Explosion => "EXPLOSION",
            Self::Crush => "CRUSH",
            Self::ArmorPiercing => "ARMOR_PIERCING",
            Self::SmallArms => "SMALL_ARMS",
            Self::Gattling => "GATTLING",
            Self::Radiation => "RADIATION",
            Self::Flame => "FLAME",
            Self::Laser => "LASER",
            Self::Sniper => "SNIPER",
            Self::Poison => "POISON",
            Self::Healing => "HEALING",
            Self::Unresistable => "UNRESISTABLE",
            Self::Water => "WATER",
            Self::Deploy => "DEPLOY",
            Self::Surrender => "SURRENDER",
            Self::Hack => "HACK",
            Self::KillPilot => "KILL_PILOT",
            Self::Penalty => "PENALTY",
            Self::Falling => "FALLING",
            Self::Melee => "MELEE",
            Self::Disarm => "DISARM",
            Self::HazardCleanup => "HAZARD_CLEANUP",
            Self::ParticleBeam => "PARTICLE_BEAM",
            Self::Toppling => "TOPPLING",
            Self::InfantryMissile => "INFANTRY_MISSILE",
            Self::AuroraBomb => "AURORA_BOMB",
            Self::LandMine => "LAND_MINE",
            Self::JetMissiles => "JET_MISSILES",
            Self::StealthjetMissiles => "STEALTHJET_MISSILES",
            Self::MolotovCocktail => "MOLOTOV_COCKTAIL",
            Self::ComancheVulcan => "COMANCHE_VULCAN",
            Self::SubdualMissile => "SUBDUAL_MISSILE",
            Self::SubdualVehicle => "SUBDUAL_VEHICLE",
            Self::SubdualBuilding => "SUBDUAL_BUILDING",
            Self::SubdualUnresistable => "SUBDUAL_UNRESISTABLE",
            Self::Microwave => "MICROWAVE",
            Self::KillGarrisoned => "KILL_GARRISONED",
            Self::Status => "STATUS",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_damage_type_is_found_by_its_name_of_either_case_and_holds_its_rules() {
        assert_eq!(DamageType::ALL.len(), 38);
        for (at, kind) in DamageType::ALL.into_iter().enumerate() {
            assert_eq!(kind.index(), at);
            assert_eq!(DamageType::of(kind.name()), Some(kind));
        }
        assert_eq!(DamageType::of("small_arms"), Some(DamageType::SmallArms));
        assert_eq!(DamageType::of("FROST"), None);
        assert_eq!(
            DamageType::SubdualVehicle.kind(),
            DeclaredName::new("subdual_vehicle").unwrap()
        );
        let unresisted: Vec<_> = DamageType::ALL
            .into_iter()
            .filter(|kind| !kind.resisted())
            .collect();
        assert_eq!(
            unresisted,
            [DamageType::Unresistable, DamageType::SubdualUnresistable]
        );
        let no_life: Vec<_> = DamageType::ALL
            .into_iter()
            .filter(|kind| !kind.takes_life())
            .collect();
        assert_eq!(
            no_life,
            [
                DamageType::KillPilot,
                DamageType::SubdualMissile,
                DamageType::SubdualVehicle,
                DamageType::SubdualBuilding,
                DamageType::SubdualUnresistable,
                DamageType::KillGarrisoned,
                DamageType::Status,
            ]
        );
    }
}
