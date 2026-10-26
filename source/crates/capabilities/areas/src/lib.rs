// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]
#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

use bevy_ecs::schedule::SystemSet;
use bevy_ecs::world::World;
use campfire_actions::{ActionTarget, ActionsSystems, DeliveryAims};
use campfire_common::EnumAll;
use campfire_declarations::{ById, Declarations, InstallCx, Module, ModuleSystems, SetPlace};
use campfire_deliveries::{Deliverers, Delivering, Launcher};
use campfire_math::Vec3;
use campfire_sim::{Capability, SimSystems, StableId};
use campfire_stats::{ModifierAppliers, StatsSystems};
use campfire_units::{DeliveryEntity, UnitGrid, UnitTypes};
use campfire_values::{EngineTag, TagProperties, UnitType};

use crate::area_launches::AreaLaunch;

mod area;
mod area_books;
mod area_data;
mod area_delivery_data;
mod area_holds;
mod area_launches;
mod area_life;
mod area_reach;
mod area_spec;
mod areas_api;
mod areas_effect;
mod error;
mod land_effect_data;

pub use crate::area::{Area, AreaTrigger};
pub use crate::area_data::{AreaData, AreaInsideData};
pub use crate::area_delivery_data::AreaDeliveryData;
pub use crate::area_holds::AreaHolds;
pub use crate::area_launches::AreaLaunches;
pub use crate::area_life::AreaLife;
pub use crate::area_reach::AreaReach;
pub use crate::area_spec::{AreaSpec, Inside};
pub use crate::areas_api::AreasApi;
pub use crate::error::{AreaDeliveryError, AreaError};
pub use crate::land_effect_data::LandEffectData;

/// The `areas` capability: area units, which actions deliver. It needs stats, whose modifiers an
/// area holds on the units inside it.
#[derive(Debug)]
pub struct Areas;

/// The steps of `areas`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll)]
pub enum AreasSystems {
    /// Areas trigger and end.
    Trigger,
    /// The areas the tick's actions deliver land.
    Land,
    /// The areas the tick's resolving actions deliver land.
    LandAtResolve,
    /// After the dead units' modifiers end and before stats hold: each area lists those it
    /// holds on the units inside.
    HoldInside,
}

impl ModuleSystems for AreasSystems {
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        match self {
            Self::Trigger => SetPlace::at(ActionsSystems::Trigger),
            Self::Land => SetPlace::at(ActionsSystems::Land),
            Self::LandAtResolve => SetPlace::at(ActionsSystems::Resolve),
            Self::HoldInside => (SetPlace::stage(SimSystems::Resolve)
                .after(StatsSystems::ClearDead))
            .before(StatsSystems::Hold),
        }
    }
}

impl Module for Areas {
    const CAPABILITY: Option<Capability> = Some(Capability::Areas);
    const NEEDS: &'static [Capability] = &[Capability::Stats];
    const USES: &'static [Capability] = &[];
    type Systems = AreasSystems;

    fn declare<D: Declarations>(d: &mut D) {
        d.section::<AreaData>();
        d.engine_tags(&[Self::TAG]);
        d.component::<Area>();
        d.timed::<Area>();
        d.component::<AreaTrigger>();
        d.state::<DeliveryEntity, AreaTrigger>();
        d.scratch::<AreaLaunches>();
        d.callback::<Deliverers>(Capability::Areas, Self::deliver);
        d.listed::<LandEffectData>();
        d.listed::<AreaDeliveryData>();
        d.extend::<DeliveryAims>(Self::aims);
        d.extend::<UnitTypes>(|cx, types| {
            types.tag_section::<AreaData>(cx, Self::TAG.tag());
            Ok(())
        });
        d.book::<ById<UnitType, AreaSpec>>(AreaSpec::book);
        d.rule(Self::deliveries_rule);
        d.extend::<ModifierAppliers>(Self::inside_ways);
        d.api(AreasApi::register);
    }

    /// Adds areas to a match. In Hit, after projectiles fly, each area due triggers, and its
    /// reaches and its end run their action's `on_hit` and `on_end`, or its launch's; after the
    /// tick's projectiles launch, its areas land. In Resolve, the areas the damage pass launched
    /// land, and then, before stats hold their modifiers, each area lists those it holds on the
    /// units inside. A launch queues as an effect of `areas`. A predicting client predicts no
    /// delivery, so it lands none and triggers none.
    fn install(cx: &mut InstallCx<'_, Self::Systems>) {
        cx.server(
            AreasSystems::Trigger,
            UnitGrid::before_reading(AreaLife::trigger),
        );
        cx.server(AreasSystems::Land, AreaLife::land);
        cx.server(AreasSystems::LandAtResolve, AreaLife::land);
        cx.add(AreasSystems::HoldInside, AreaHolds::hold_inside);
    }
}

impl Areas {
    /// The engine tag of area types: a filter selects their units only when it names it.
    pub const TAG: EngineTag = EngineTag::new(3, "area", TagProperties::NONE.with_named_only());

    /// Lands the area of `launcher`, which aimed at `target` from `from`: on the point it aimed
    /// at, where the unit it aimed at stands, or at `from` for an action that aims at nothing;
    /// nothing for a unit that is gone.
    fn deliver(world: &mut World, launcher: Launcher, from: Vec3, target: ActionTarget) {
        let Launcher { by, delivery } = launcher;
        let unit_type = delivery.unit_type;
        let at = match target.position(world) {
            Some(at) => at,
            None if target == ActionTarget::None => from,
            None => return,
        };
        Self::push(world, by, unit_type, at, target.unit(), None);
    }

    /// Queues an area of `unit_type` of `by` to land at `position`, aimed at `aimed`; with `id`,
    /// the id the script that placed it took for it.
    fn push(
        world: &mut World,
        by: Delivering,
        unit_type: UnitType,
        position: Vec3,
        aimed: Option<StableId>,
        id: Option<StableId>,
    ) {
        world.resource_mut::<AreaLaunches>().0.push(AreaLaunch {
            id,
            by,
            position,
            unit_type,
            aimed,
        });
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    use bevy_ecs::world::World;
    use campfire_common::DeclaredName;
    use campfire_declarations::ById;
    use campfire_declarations::internals::BookLoad;
    use campfire_script::UnknownName;
    use campfire_sim::MatchRate;
    use campfire_stats::Stats;
    use campfire_units::{UnitTypeBook, Units};
    use campfire_values::{ModifierId, Named, PackageIndex, UnitType};

    use crate::Areas;
    use crate::area_data::AreaData;
    use crate::area_spec::AreaSpec;

    impl Areas {
        /// Makes `unit_type` of `package` an area type of `data`, tagged `area`, its times in
        /// ticks at the match's rate, rounded up: its `affects` filter names tags of the match's,
        /// and its `inside` modifiers of the package.
        pub fn load_type(
            world: &mut World,
            unit_type: UnitType,
            package: PackageIndex,
            data: &AreaData,
        ) {
            let rate = *world.resource::<MatchRate>();
            Units::change_types(world, |_, types| types.give_tag(unit_type, Self::TAG.tag()));
            let modifier = |name: &DeclaredName| {
                Stats::modifier(world, package, name.as_str())
                    .ok_or_else(|| UnknownName::new(ModifierId::NAMES, name))
            };
            let tags = world.resource::<UnitTypeBook>().tags();
            let spec = AreaSpec::of(data, tags, rate, modifier)
                .expect("a test's area holds in ticks and names what its match has");
            ById::<UnitType, AreaSpec>::amend(world, |_, book| book.set(unit_type, spec));
        }
    }
}
