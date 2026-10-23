use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declarations::{ById, KindSpawn};
use campfire_math::RngStream;
use campfire_sim::{SimComponent, SimRng, StableId};
use campfire_states::{Replication, SentOnce};
use campfire_values::UnitType;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::unit_entity::UnitEntity;
use crate::variations::Variations;

/// Which of its type's variations a unit is, its place among them, as its spawn drew it: state,
/// which every client receives once, as it draws the variation's look
/// (04-capabilities/00-overview.md, A unit type may vary its units).
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable)]
pub struct Variation(u8);

impl Variation {
    /// The stream a spawn draws its unit's variation on, the unit's own.
    const STREAM: RngStream = RngStream::new("units.variation");

    pub const fn new(place: u8) -> Self {
        Self(place)
    }

    pub const fn get(self) -> u8 {
        self.0
    }

    /// The variation of the unit `id` of `unit_type` that spawns in `world`, drawn on its own
    /// stream; none for a type of no variations.
    pub(crate) fn draw(world: &World, id: StableId, unit_type: UnitType) -> Option<Self> {
        let variations = world.get_resource::<ById<UnitType, Variations>>()?;
        let count = variations.get(unit_type)?.len();
        let count = u64::try_from(count).expect("a type's variations fit u64");
        let at = world
            .resource::<SimRng>()
            .open(Self::STREAM, id)
            .below(count);
        Some(Self(
            u8::try_from(at).expect("a type holds at most 256 variations"),
        ))
    }

    /// Whether it is one of the variations of `unit_type` in `world`.
    pub(crate) fn fits(self, world: &World, unit_type: UnitType) -> bool {
        let variations = world.get_resource::<ById<UnitType, Variations>>();
        let of_type = variations.and_then(|book| book.get(unit_type));
        of_type.is_some_and(|variations| usize::from(self.0) < variations.len())
    }

    /// The variation of the unit `at` spawns, its core's; as a restore checks it, its own, or for
    /// a unit of a type that varies and holds none, the first, as the check reads only whether
    /// its spawn gives one, so it refuses the unit.
    pub(crate) fn of<H>(at: &KindSpawn<'_, H>) -> Option<Self> {
        if let Some(core) = at.core() {
            return core.variation.map(Self);
        }
        at.of_type::<Variations>()?;
        Some(
            at.world()
                .get::<Self>(at.entity())
                .copied()
                .unwrap_or(Self(0)),
        )
    }
}

impl SimComponent for Variation {
    const NAME: &'static str = "units.variation";
    type Host = UnitEntity;

    /// One of its type's variations, which a restore's untrusted snapshot may not name.
    fn check(&self, world: &World, entity: Entity) -> bool {
        let unit_type = world.get::<UnitType>(entity);
        unit_type.is_some_and(|&unit_type| self.fits(world, unit_type))
    }
}

impl Replication for Variation {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnce;
}
