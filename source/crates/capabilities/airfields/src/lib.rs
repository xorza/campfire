//! Airfields: the places and runways of the buildings that park units, and each parked unit's
//! sortie from its place into the air (04-capabilities/airfields.md, An airfield is a capability).

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use campfire_declarations::{ById, Declarations, InstallCx, Module, ModuleSystems, SetPlace};
use campfire_production::ProductionSystems;
use campfire_sim::{Capability, SimSystems};
use campfire_units::{BodyUnit, SpawnCallbacks};
use campfire_values::{Block, EngineTag, TagProperties, UnitType};

use crate::sorties::Sorties;

mod airfield_data;
mod error;
mod runways;
mod sortie;
mod sortie_data;
mod sorties;

pub use crate::airfield_data::{AirfieldData, AirfieldPlace, ApproachData, RunwayData};
pub use crate::error::AirfieldDataError;
pub use crate::runways::Runways;
pub use crate::sortie::Sortie;
pub use crate::sortie_data::{SortieData, SortieSpec, StrandedData, StrandedLoss};

/// The `airfields` capability: a building's places and runways, and the sorties of the units it
/// parks there; it needs production, whose places it reads, and navigation, which flies its
/// units (04-capabilities/airfields.md, An airfield is a capability).
#[derive(Debug)]
pub struct Airfields;

/// Airfields' steps.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AirfieldsSystems {
    /// Parked units go for their runways, taxi and roll, after the orders and trains start and
    /// before the units move.
    Sorties,
}

impl ModuleSystems for AirfieldsSystems {
    const ALL: &'static [Self] = &[Self::Sorties];

    fn place(self) -> SetPlace {
        match self {
            Self::Sorties => SetPlace::stage(SimSystems::Act).after(ProductionSystems::Start),
        }
    }
}

impl Module for Airfields {
    const CAPABILITY: Option<Capability> = Some(Capability::Airfields);
    const NEEDS: &'static [Capability] = &[Capability::Production, Capability::Navigation];
    const USES: &'static [Capability] = &[Capability::Combat];
    type Systems = AirfieldsSystems;

    fn declare<D: Declarations>(d: &mut D) {
        d.section::<AirfieldData>();
        d.section::<SortieData>();
        d.engine_tags(&[Self::HELD]);
        d.book::<ById<UnitType, AirfieldData>>(AirfieldData::book);
        d.book::<ById<UnitType, SortieSpec>>(SortieSpec::book);
        d.component::<Runways>();
        d.component::<Sortie>();
        d.part::<BodyUnit, Runways>(|at| {
            at.of_type::<AirfieldData>()
                .map(|airfield| Runways::free_of(airfield.runways.len()))
        });
        d.part::<BodyUnit, Sortie>(|at| at.of_type::<SortieSpec>().map(|_| Sortie::FLYING));
        d.callback::<SpawnCallbacks>(Capability::Airfields, Sorties::park);
    }

    /// Adds airfields to a match: in Act, after the trains start and before the units move, the
    /// runways of the units that died go free, and each unit of a sortie steps its way from its
    /// place into the air and back, on the server and in a predicting client alike; then, on the
    /// server, the units an airfield that died held die with it, and each airfield heals the units
    /// that rest at it while each stranded unit loses its life.
    fn install(cx: &mut InstallCx<'_, Self::Systems>) {
        cx.add(
            AirfieldsSystems::Sorties,
            (Sorties::free_runways, Sorties::step).chain(),
        );
        cx.server(
            AirfieldsSystems::Sorties,
            (Sorties::kill_held, Sorties::upkeep)
                .chain()
                .after(Sorties::step),
        );
    }
}

impl Airfields {
    /// The engine tag of a unit its airfield holds, from its runway's approach as it lands to its
    /// runway's end as it takes off: it blocks its walk and its attacks, so its orders wait for
    /// the air.
    pub const HELD: EngineTag = EngineTag::new(
        11,
        "airfield_held",
        TagProperties::NONE
            .with_block(Block::Move)
            .with_block(Block::Attack),
    );
}
