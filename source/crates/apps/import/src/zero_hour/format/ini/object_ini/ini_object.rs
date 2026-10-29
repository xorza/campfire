use campfire_client_files::ModelScale;

use crate::zero_hour::format::game_frames;
use crate::zero_hour::format::ini::object_ini::draw_module::DrawModule;
use crate::zero_hour::format::ini::object_ini::fight_data::FightData;
use crate::zero_hour::format::ini::object_ini::geometry::Geometry;
use crate::zero_hour::format::ini::object_ini::ini_build::IniBuild;
use crate::zero_hour::format::ini::object_ini::ini_contain::{ContainClass, IniContain};
use crate::zero_hour::format::ini::object_ini::ini_death::IniDeath;
use crate::zero_hour::format::ini::object_ini::ini_experience::IniExperience;
use crate::zero_hour::format::ini::object_ini::ini_fall::IniFall;
use crate::zero_hour::format::ini::object_ini::ini_power::IniPowers;
use crate::zero_hour::format::ini::object_ini::ini_propaganda::IniPropaganda;
use crate::zero_hour::format::ini::object_ini::ini_supply::IniSupply;
use crate::zero_hour::format::ini::object_ini::ini_upgrade_module::{
    IniUpgradeModule, UpgradeEffect,
};
use crate::zero_hour::format::ini::object_ini::ini_voices::IniVoices;
use crate::zero_hour::format::ini::object_ini::object_kinds::{ObjectKind, ObjectKinds};
use crate::zero_hour::format::ini::object_ini::{IniFile, ObjectId};

/// An object as the game holds it once read, a reskin a copy of its base with its own fields
/// over it (`ThingTemplate::copyFrom`): its name as the INI writes it, its draw modules, and the
/// fields that say how the game draws, places and moves it: its kinds, its fence's width, its
/// asset scale, whether it is a landmark bridge, its geometry and its normal locomotors by name,
/// each the game's default when absent; and its upgrade modules, in the order it gives them.
#[derive(Debug, Clone)]
pub(crate) struct IniObject {
    pub(crate) id: ObjectId,
    pub(crate) name: String,
    /// The file that defines it, a reskin's own.
    pub(super) file: IniFile,
    pub(super) draws: Vec<DrawModule>,
    pub(super) kinds: ObjectKinds,
    pub(super) fence_width: f32,
    pub(super) scale: ModelScale,
    pub(super) is_bridge: bool,
    pub(super) geometry: Geometry,
    /// The locomotors of its `SET_NORMAL`, in the order its line names them, and of its
    /// `SET_TAXIING`, by which a jet moves on the ground.
    pub(super) locomotors: Vec<String>,
    pub(super) taxi_locomotors: Vec<String>,
    pub(super) build: IniBuild,
    pub(super) supply: IniSupply,
    pub(super) fight: FightData,
    pub(super) upgrades: Vec<IniUpgradeModule>,
    pub(super) deaths: Vec<IniDeath>,
    /// Its `PhysicsBehavior`'s `Mass`, 1 when it has none, as `DEFAULT_MASS`.
    pub(super) mass: f32,
    /// Its `PhysicsBehavior`'s `LateralFriction`, a share a second, none when it has none.
    pub(super) lateral_friction: Option<f32>,
    /// Its `LifetimeUpdate`'s `MinLifetime` and `MaxLifetime`, in milliseconds.
    pub(super) lifetime_ms: Option<[f32; 2]>,
    /// Its `PropagandaTowerBehavior`, none when it has none.
    pub(super) propaganda: Option<IniPropaganda>,
    pub(super) voices: IniVoices,
    /// Its `VisionRange`, 0 when absent, and its `ShroudClearingRange`, -1 when absent, as
    /// `ThingTemplate` starts them.
    pub(super) vision_range: f32,
    pub(super) shroud_clearing_range: f32,
    /// The upgrade each of its `GrantUpgradeCreate`s grants it as it is made.
    pub(super) granted: Vec<String>,
    /// Its `RadarPriority`.
    pub(super) radar_priority: RadarPriority,
    /// Its contain module, its last when it gives several, as `Object` keeps the last, and the
    /// room it takes of a transport, its `TransportSlotCount`.
    pub(super) contain: Option<IniContain>,
    pub(super) transport_slots: i32,
    /// The mapped image a button that stands for it shows, its `ButtonImage`, none of none.
    pub(super) button_image: Option<String>,
    pub(super) experience: IniExperience,
    pub(super) powers: IniPowers,
}

/// An object's `RadarPriority` (`RadarPriorityNames`): none set, `INVALID`; one the radar draws
/// for no one; one it draws for its controller alone; or one it draws.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum RadarPriority {
    #[default]
    Unset,
    NotOnRadar,
    LocalUnitOnly,
    Shown,
}

/// Who draws an object a map places, as a multiplayer game spawns it (09-zero-hour.md, The
/// multiplayer rule for client objects): a unit of the sim placed before every other, as the game
/// spawns bridge-like objects in a pass of their own; the client's tree buffer; its prop buffer; or
/// the sim, as a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlacedAs {
    Bridge,
    Tree,
    Prop,
    Unit,
}

impl IniObject {
    pub(crate) fn draws(&self) -> &[DrawModule] {
        &self.draws
    }

    pub(crate) const fn file(&self) -> IniFile {
        self.file
    }

    /// Whether `other` differs from it in its body and its look alone: its geometry, its draw
    /// modules and its scale (09-zero-hour.md, A variation varies the body and the look alone).
    pub(crate) fn varies_alone(&self, other: &Self) -> bool {
        self.kinds == other.kinds
            && self.fence_width == other.fence_width
            && self.is_bridge == other.is_bridge
            && self.locomotors == other.locomotors
            && self.taxi_locomotors == other.taxi_locomotors
            && self.build == other.build
            && self.supply == other.supply
            && self.fight == other.fight
    }

    /// Who draws an object of it a map places: a bridge-like object, a landmark bridge or a wall
    /// a unit walks on, the sim before every other; an optimized tree the tree buffer; a prop, or
    /// fluff, an object a build clears whose fence is 0 wide, the prop buffer, as a multiplayer
    /// game forces fluff onto it; any other the sim.
    pub(crate) const fn placed_as(&self) -> PlacedAs {
        let kinds = self.kinds;
        if self.is_bridge || kinds.holds(ObjectKind::WalkOnTopOfWall) {
            PlacedAs::Bridge
        } else if kinds.holds(ObjectKind::OptimizedTree) {
            PlacedAs::Tree
        } else if kinds.holds(ObjectKind::Prop)
            || (kinds.holds(ObjectKind::ClearedByBuild) && self.fence_width == 0.0)
        {
            PlacedAs::Prop
        } else {
            PlacedAs::Unit
        }
    }

    /// The geometry of its `Geometry` fields.
    pub(crate) const fn geometry(&self) -> Geometry {
        self.geometry
    }

    /// The names of the locomotors it moves by, its `SET_NORMAL`'s.
    pub(crate) fn locomotors(&self) -> &[String] {
        &self.locomotors
    }

    /// The names of the locomotors it taxis by, its `SET_TAXIING`'s.
    pub(crate) fn taxi_locomotors(&self) -> &[String] {
        &self.taxi_locomotors
    }

    /// What it gives production: its cost, time, command set, prerequisites and modules.
    pub(crate) const fn build(&self) -> &IniBuild {
        &self.build
    }

    /// What it gives supply: its dock, its center, its gatherer and its spawns.
    pub(crate) const fn supply(&self) -> &IniSupply {
        &self.supply
    }

    /// What it gives combat: its body, its weapon sets, its armor sets, its shell and its turret.
    pub(crate) const fn fight(&self) -> &FightData {
        &self.fight
    }

    /// The voices it answers with.
    pub(crate) const fn voices(&self) -> &IniVoices {
        &self.voices
    }

    /// The slow death a normal death of it most likely takes, as `SlowDeathBehavior::onDie` draws
    /// one by weight among those that apply: of the most weight, the first of a tie; none when
    /// none applies, as it is then destroyed at once.
    pub(crate) fn slow_death(&self) -> Option<&IniDeath> {
        let applies = self
            .deaths
            .iter()
            .filter(|death| death.effect.is_none() && death.fall.is_none() && death.applies());
        applies.rev().max_by_key(|death| death.weight)
    }

    /// The fall a normal death of it takes, its first aircraft's slow death that applies; none
    /// when none does.
    pub(crate) fn fall(&self) -> Option<&IniFall> {
        self.fall_death()?.fall.as_ref()
    }

    /// The aircraft's slow death a normal death of it takes, its first that applies; none when
    /// none does.
    pub(crate) fn fall_death(&self) -> Option<&IniDeath> {
        let mut applies = self.deaths.iter().filter(|death| death.applies());
        applies.find(|death| death.fall.is_some())
    }

    /// The effect list a normal death of it plays: the `DeathFX` of its first `FXListDie` that
    /// applies and names one (09-zero-hour.md, Particles and effects); none when none does.
    pub(crate) fn death_effect(&self) -> Option<&str> {
        let effects = self.deaths.iter().filter(|death| death.applies());
        effects
            .filter_map(|death| death.effect.as_deref())
            .find(|effect| !effect.is_empty() && !effect.eq_ignore_ascii_case("none"))
    }

    /// Its upgrade modules, in the order it gives them.
    /// Whether it gives its player a radar as it is made: a `RadarUpgrade` of its own whose every
    /// trigger a `GrantUpgradeCreate` of its own grants, as the USA's command center and the GLA's
    /// radar van grant theirs (02-engine-core.md, A minimap by a radar tag).
    pub(crate) fn makes_radar(&self) -> bool {
        self.upgrades.iter().any(|module| {
            module.effect == UpgradeEffect::Radar
                && !module.triggers.is_empty()
                && module
                    .triggers
                    .iter()
                    .all(|trigger| self.granted.contains(trigger))
        })
    }

    /// Its priority on the radar: its `RadarPriority`, or of none set, a structure's for one a
    /// garrison holds or that is captured, as `Object::getRadarPriority` decides, else none the
    /// radar draws.
    pub(crate) const fn radar_priority(&self) -> RadarPriority {
        match self.radar_priority {
            RadarPriority::Unset
                if self.garrisons() || self.kinds.holds(ObjectKind::Capturable) =>
            {
                RadarPriority::Shown
            }
            priority => priority,
        }
    }

    pub(crate) fn upgrades(&self) -> &[IniUpgradeModule] {
        &self.upgrades
    }

    /// Its contain module, if it has one.
    pub(crate) const fn contain(&self) -> Option<&IniContain> {
        self.contain.as_ref()
    }

    /// Whether a `GarrisonContain` holds its garrison.
    pub(crate) const fn garrisons(&self) -> bool {
        matches!(
            &self.contain,
            Some(IniContain {
                class: ContainClass::Garrison,
                ..
            })
        )
    }

    /// The room it takes of a transport, its `TransportSlotCount`: 0 for one no transport takes.
    pub(crate) const fn transport_slots(&self) -> i32 {
        self.transport_slots
    }

    /// The mapped image a button that stands for it shows, its `ButtonImage`, as a transport's
    /// exit shows its rider (`ControlBar::populateButtonProc`).
    pub(crate) fn button_image(&self) -> Option<&str> {
        self.button_image.as_deref()
    }

    /// Its kinds of `KindOf` that the import reads.
    pub(crate) const fn kinds(&self) -> ObjectKinds {
        self.kinds
    }

    /// Whether it counts for a multiplayer victory: a structure of the kind
    /// `MP_COUNT_FOR_VICTORY`, as `Team::hasAnyBuildings` counts it.
    pub(crate) const fn counts_for_victory(&self) -> bool {
        self.kinds.holds(ObjectKind::Structure) && self.kinds.holds(ObjectKind::MpCountForVictory)
    }

    /// Whether it is a structure, of `KindOf`.
    pub(crate) const fn is_structure(&self) -> bool {
        self.kinds.holds(ObjectKind::Structure)
    }

    /// Whether it is a dozer, of `KindOf`.
    pub(crate) const fn is_dozer(&self) -> bool {
        self.kinds.holds(ObjectKind::Dozer)
    }

    /// Whether it is a vehicle, of `KindOf`.
    pub(crate) const fn is_vehicle(&self) -> bool {
        self.kinds.holds(ObjectKind::Vehicle)
    }

    /// Whether it may answer an ally's call to retaliate: neither `IMMOBILE`, a `DRONE` nor
    /// `CANNOT_RETALIATE`, as `AIUpdateInterface::shouldRetaliate` refuses them.
    pub(crate) const fn may_retaliate(&self) -> bool {
        !(self.kinds.holds(ObjectKind::Immobile)
            || self.kinds.holds(ObjectKind::Drone)
            || self.kinds.holds(ObjectKind::CannotRetaliate))
    }

    /// Whether it is a hulk, of `KindOf`: a wreck of a death, which no rule reads.
    pub(crate) const fn is_hulk(&self) -> bool {
        self.kinds.holds(ObjectKind::Hulk)
    }

    /// Its mass, its `PhysicsBehavior`'s, which a force pushes it by.
    pub(crate) const fn mass(&self) -> f32 {
        self.mass
    }

    /// The share of its lateral velocity it loses a frame, its `PhysicsBehavior`'s
    /// `LateralFriction` a second times the `f32` of a frame's seconds, as `parseFrictionPerSec`
    /// reads it, or `DEFAULT_LATERAL_FRICTION`, 0.15, with none.
    pub(crate) fn lateral_friction(&self) -> f32 {
        self.lateral_friction
            .map_or(0.15, |per_second| per_second * game_frames::SECONDS)
    }

    /// Its `VisionRange`, 0 when absent.
    pub(crate) const fn vision_range(&self) -> f32 {
        self.vision_range
    }

    /// How far it clears the shroud for its player: its `ShroudClearingRange`, or its
    /// `VisionRange` when it gives none, as `Object`'s constructor takes it.
    pub(crate) fn sight_range(&self) -> f32 {
        if self.shroud_clearing_range == -1.0 {
            self.vision_range
        } else {
            self.shroud_clearing_range
        }
    }

    /// Its special powers and its heal burst.
    pub(crate) const fn powers(&self) -> &IniPowers {
        &self.powers
    }

    /// Its veterancy: what its death gives, what each level needs, whether it gains any, and the
    /// levels it is made at.
    pub(crate) const fn experience(&self) -> &IniExperience {
        &self.experience
    }

    /// Its `PropagandaTowerBehavior`, none when it has none.
    pub(crate) const fn propaganda(&self) -> Option<&IniPropaganda> {
        self.propaganda.as_ref()
    }

    /// The least and the most of its lifetime in milliseconds, its `LifetimeUpdate`'s; none for
    /// an object of none.
    pub(crate) const fn lifetime_ms(&self) -> Option<[f32; 2]> {
        self.lifetime_ms
    }

    /// Whether it is a wall, which a projectile of `WALLS` detonates on: a fence, of a
    /// `FenceWidth` above 0 (`shouldProjectileCollideWith`).
    pub(crate) const fn is_wall(&self) -> bool {
        self.fence_width > 0.0
    }

    /// The asset scale every drawable of it takes: its `Scale`.
    pub(crate) const fn scale(&self) -> ModelScale {
        self.scale
    }
}
