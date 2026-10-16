#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]
#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::world::World;
use campfire_actions::{ActionTarget, EffectQueues};
use campfire_common::EnumAll;
use campfire_declare::{ByType, Declarations, InstallCx, Module, ModuleSet, Phase, SetPlace};
use campfire_deliveries::{Deliverers, Delivering, DeliveryKind, Launcher};
use campfire_math::Vec3;
use campfire_sim::{Capability, StableId};
use campfire_units::DeliveryEntity;
use campfire_values::{EngineTag, TagProperties, UnitType};

mod area;
mod area_data;
mod area_holds;
mod area_launches;
mod area_life;
mod area_reach;
mod area_spec;
mod area_trigger;
mod areas_api;
mod areas_effect;
mod error;

pub use crate::area::Area;
pub use crate::area_data::{AreaData, AreaInsideData};
pub use crate::area_holds::AreaHolds;
pub use crate::area_launches::{AreaLaunch, AreaLaunches};
pub use crate::area_life::AreaLife;
pub use crate::area_reach::AreaReach;
pub use crate::area_spec::{AreaSpec, Inside};
pub use crate::area_trigger::AreaTrigger;
pub use crate::areas_api::AreasApi;
pub use crate::areas_effect::AreasEffect;
pub use crate::error::AreaError;

/// The `areas` capability: area units, which actions deliver. It needs stats, whose modifiers an
/// area holds on the units inside it.
#[derive(Debug)]
pub struct Areas;

/// The steps of `areas`.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAll)]
pub(crate) enum AreasSet {
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

impl ModuleSet for AreasSet {
    const CAPABILITY: Option<Capability> = Some(Capability::Areas);
    const ALL: &'static [Self] = &Self::ALL;

    fn place(self) -> SetPlace {
        SetPlace::Phase(match self {
            Self::Trigger => Phase::Trigger,
            Self::Land => Phase::Land,
            Self::LandAtResolve => Phase::Resolve,
            Self::HoldInside => Phase::HoldInside,
        })
    }
}

impl Module for Areas {
    const CAPABILITY: Option<Capability> = Some(Capability::Areas);
    const NEEDS: &'static [Capability] = &[Capability::Stats];
    const USES: &'static [Capability] = &[];

    fn declare<D: Declarations>(d: &mut D) {
        d.sets::<AreasSet>();
        d.engine_tags(&[Self::TAG]);
        d.component::<Area>();
        d.component::<AreaTrigger>();
        d.state::<DeliveryEntity, AreaTrigger>();
        d.scratch::<AreaLaunches>();
        d.callback::<Deliverers>(DeliveryKind::Area, Self::deliver);
        d.callback::<EffectQueues>(Capability::Areas, AreasEffect::queue_listed);
        d.book::<ByType<AreaSpec>>();
        d.api(AreasApi::register);
    }

    /// Adds areas to a match. In Hit, after projectiles fly, each area due triggers, and its
    /// reaches and its end run their action's `on_hit` and `on_end`, or its launch's; after the
    /// tick's projectiles launch, its areas land. In Resolve, the areas the damage pass launched
    /// land, and then, before stats hold their modifiers, each area lists those it holds on the
    /// units inside. A launch queues as an effect of `areas`. A predicting client predicts no
    /// delivery, so it triggers none.
    fn install(cx: &mut InstallCx<'_>) {
        cx.server(AreaLife::trigger.in_set(AreasSet::Trigger));
        cx.systems((
            AreaLife::land.in_set(AreasSet::Land),
            AreaLife::land.in_set(AreasSet::LandAtResolve),
            AreaHolds::hold_inside.in_set(AreasSet::HoldInside),
        ));
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
        let at = match target.pos(world) {
            Some(at) => at,
            None if target == ActionTarget::None => from,
            None => return,
        };
        Self::push(world, by, unit_type, at, target.unit(), None);
    }

    /// Queues an area of `unit_type` of `by` to land at `pos`, aimed at `aimed`; with `id`, the id
    /// the script that placed it took for it.
    fn push(
        world: &mut World,
        by: Delivering,
        unit_type: UnitType,
        pos: Vec3,
        aimed: Option<StableId>,
        id: Option<StableId>,
    ) {
        world.resource_mut::<AreaLaunches>().0.push(AreaLaunch {
            id,
            by,
            pos,
            unit_type,
            aimed,
        });
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    use bevy_ecs::world::World;
    use campfire_declare::ByType;
    use campfire_script::{NameKind, UnknownName};
    use campfire_sim::TickRate;
    use campfire_stats::Stats;
    use campfire_units::{UnitTypeBook, Units};
    use campfire_values::{DeclaredName, PackageIndex, UnitType};

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
            let rate = *world.resource::<TickRate>();
            Units::change_types(world, |_, types| types.give_tag(unit_type, Self::TAG.tag()));
            let modifier = |name: &DeclaredName| {
                Stats::modifier(world, package, name.as_str())
                    .ok_or_else(|| UnknownName::new(NameKind::Modifier, name))
            };
            let tags = world.resource::<UnitTypeBook>().tags();
            let spec = AreaSpec::of(data, tags, rate, modifier)
                .expect("a test's area holds in ticks and names what its match has");
            world
                .resource_mut::<ByType<AreaSpec>>()
                .set(unit_type, spec);
        }
    }
}
