use campfire_values::DeclaredName;

campfire_common::enum_all! {
/// Zero Hour's damage types, in the order of `DamageTypeFlags`' names: Generals' `FLESHY_SNIPER`
/// is not among them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
/// Every type, in order.
pub(crate) const ALL;
}

impl DamageType {
    /// The type `name`, of either case, as `INI::scanIndexList` finds it.
    pub(crate) fn of(name: &str) -> Option<DamageType> {
        DamageType::ALL
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
        !matches!(
            self,
            DamageType::Unresistable | DamageType::SubdualUnresistable
        )
    }

    /// Whether it takes life, as `IsHealthDamagingDamage` holds: a status, the four subdual
    /// types, which disable, and the kill of a vehicle's pilot or of a building's garrison take
    /// none. `HEALING` takes life here, as the function says, though `ActiveBody::attemptDamage`
    /// gives it to its target.
    pub(crate) const fn takes_life(self) -> bool {
        !matches!(
            self,
            DamageType::Status
                | DamageType::SubdualMissile
                | DamageType::SubdualVehicle
                | DamageType::SubdualBuilding
                | DamageType::SubdualUnresistable
                | DamageType::KillPilot
                | DamageType::KillGarrisoned
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
            DamageType::Explosion => "EXPLOSION",
            DamageType::Crush => "CRUSH",
            DamageType::ArmorPiercing => "ARMOR_PIERCING",
            DamageType::SmallArms => "SMALL_ARMS",
            DamageType::Gattling => "GATTLING",
            DamageType::Radiation => "RADIATION",
            DamageType::Flame => "FLAME",
            DamageType::Laser => "LASER",
            DamageType::Sniper => "SNIPER",
            DamageType::Poison => "POISON",
            DamageType::Healing => "HEALING",
            DamageType::Unresistable => "UNRESISTABLE",
            DamageType::Water => "WATER",
            DamageType::Deploy => "DEPLOY",
            DamageType::Surrender => "SURRENDER",
            DamageType::Hack => "HACK",
            DamageType::KillPilot => "KILL_PILOT",
            DamageType::Penalty => "PENALTY",
            DamageType::Falling => "FALLING",
            DamageType::Melee => "MELEE",
            DamageType::Disarm => "DISARM",
            DamageType::HazardCleanup => "HAZARD_CLEANUP",
            DamageType::ParticleBeam => "PARTICLE_BEAM",
            DamageType::Toppling => "TOPPLING",
            DamageType::InfantryMissile => "INFANTRY_MISSILE",
            DamageType::AuroraBomb => "AURORA_BOMB",
            DamageType::LandMine => "LAND_MINE",
            DamageType::JetMissiles => "JET_MISSILES",
            DamageType::StealthjetMissiles => "STEALTHJET_MISSILES",
            DamageType::MolotovCocktail => "MOLOTOV_COCKTAIL",
            DamageType::ComancheVulcan => "COMANCHE_VULCAN",
            DamageType::SubdualMissile => "SUBDUAL_MISSILE",
            DamageType::SubdualVehicle => "SUBDUAL_VEHICLE",
            DamageType::SubdualBuilding => "SUBDUAL_BUILDING",
            DamageType::SubdualUnresistable => "SUBDUAL_UNRESISTABLE",
            DamageType::Microwave => "MICROWAVE",
            DamageType::KillGarrisoned => "KILL_GARRISONED",
            DamageType::Status => "STATUS",
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
