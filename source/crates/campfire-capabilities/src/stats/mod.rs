//! The `stats` capability: a unit type's stats and how they grow by level, the rules the mode
//! declares for them, and each unit's stats derived from its type and level.

use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};
use bevy_ecs::world::World;
use campfire_common::Ticks;
use campfire_sim::{Capability, SimEdge, SimSet, SimTick, SimWorld, StableId, TickRate};

use crate::declare::declarations::Declarations;
use crate::declare::declares::Declares;
use crate::declare::install_cx::InstallCx;
use crate::players::player::Player;
use crate::stats::applier::Applier;
use crate::stats::carried_mut::CarriedMut;
use crate::stats::held_modifiers::HeldModifiers;
use crate::stats::held_pass::HeldPass;
use crate::stats::level::Level;
use crate::stats::live_carriers::LiveCarriers;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::modifier_clocks::ModifierClocks;
use crate::stats::modifier_ends::ModifierEnds;
use crate::stats::modifier_handle::ModifierHandle;
use crate::stats::modifier_spec::ParamPlace;
use crate::stats::modifiers::Modifiers;
use crate::stats::param_book::ParamBook;
use crate::stats::param_source::ParamSource;
use crate::stats::player_modifiers::PlayerModifiers;
use crate::stats::pool_book::PoolBook;
use crate::stats::pools::Pools;
use crate::stats::refresh::Refresh;
use crate::stats::stat_book::StatBook;
use crate::stats::stats_api::StatsApi;
use crate::stats::stats_call::StatsCall;
use crate::stats::stats_column::{RowParts, StatsColumn};
use crate::stats::unit_stats::UnitStats;
use crate::units::action_id::ActionId;
use crate::units::dead::Dead;
use crate::units::modifier_id::ModifierId;
use crate::units::tag::Tag;
use crate::units::view::View;
use crate::values::rank::Rank;

pub(crate) mod application;
pub(crate) mod applier;
pub(crate) mod carried_mut;
pub(crate) mod error;
pub(crate) mod held_modifiers;
pub(crate) mod held_pass;
pub(crate) mod instance;
pub(crate) mod level;
pub(crate) mod life;
pub(crate) mod life_pool;
pub(crate) mod lifetime;
pub(crate) mod live_carriers;
pub(crate) mod live_param;
pub(crate) mod meter;
pub(crate) mod modifier_book;
pub(crate) mod modifier_clocks;
pub(crate) mod modifier_data;
pub(crate) mod modifier_ends;
pub(crate) mod modifier_handle;
pub(crate) mod modifier_params;
pub(crate) mod modifier_spec;
pub(crate) mod modifier_state_field;
pub(crate) mod modifiers;
pub(crate) mod param_book;
pub(crate) mod param_read;
pub(crate) mod param_source;
pub(crate) mod param_table;
pub(crate) mod player_modifiers;
pub(crate) mod pool_book;
pub(crate) mod pool_cost;
pub(crate) mod pool_data;
pub(crate) mod pool_id;
pub(crate) mod pools;
pub(crate) mod refresh;
pub(crate) mod refresh_scratch;
pub(crate) mod stat_book;
pub(crate) mod stat_books;
pub(crate) mod stat_change;
pub(crate) mod stat_graph;
pub(crate) mod stat_id;
pub(crate) mod stat_op;
pub(crate) mod stat_rule;
pub(crate) mod stat_totals;
pub(crate) mod stats_api;
pub(crate) mod stats_call;
pub(crate) mod stats_column;
pub(crate) mod stats_data;
pub(crate) mod stats_effect;
pub(crate) mod stats_world;
pub(crate) mod unit_pools;
pub(crate) mod unit_stats;

/// The `stats` capability.
#[derive(Debug)]
pub struct Stats;

/// The systems of `stats`, for the capabilities built on it to order theirs against.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum StatsSet {
    /// In `SimSet::Inputs`: the modifiers and stacks that hold no longer end.
    Expire,
    /// In `SimSet::Inputs`: the living units' pools regenerate.
    Regenerate,
    /// In `SimSet::Resolve`, after the tick's deaths: the dead units' modifiers end, then
    /// auras, players and `HeldModifiers` hold their modifiers.
    Hold,
}

impl Declares for Stats {
    const MODULE: Option<Capability> = Some(Capability::Stats);

    fn declare<D: Declarations>(d: &mut D) {
        d.component::<PlayerModifiers>();
        d.component::<Level>();
        d.component::<Modifiers>();
        d.component::<ModifierClocks>();
        d.component::<Pools>();
        d.derived::<UnitStats>();
        d.part::<Player, PlayerModifiers>(|_| PlayerModifiers::default());
        d.book::<ModifierBook>();
        d.column::<StatsColumn, RowParts>(StatsColumn::fill_row);
        d.call_part::<StatsCall>();
        d.api(StatsApi::register);
    }
}

impl Stats {
    /// Adds stats to a match: in `SimEdge::Start` and in the `SimEdge::After` of each stage,
    /// every unit whose level or modifiers changed, or that is new, has its stats and states
    /// derived again, and the components that hold their effect follow them; as each tick
    /// starts, the living units' pools regenerate. With no stat book, as before a mode loads one
    /// or on a client, which loads none, nothing changes.
    pub(crate) fn install(cx: &mut InstallCx<'_>) {
        cx.world.insert_resource(HeldModifiers::default());
        cx.world.insert_resource(LiveCarriers::default());
        let schedule = &mut *cx.schedule;
        schedule.add_systems((
            (
                ModifierEnds::expire_modifiers.in_set(StatsSet::Expire),
                Refresh::regenerate.in_set(StatsSet::Regenerate),
            )
                .in_set(SimSet::Inputs),
            (ModifierEnds::clear_dead_modifiers, HeldPass::run)
                .chain()
                .in_set(SimSet::Resolve)
                .in_set(StatsSet::Hold),
            (Refresh::give_parts, Refresh::run)
                .chain()
                .in_set(SimEdge::Start),
        ));
        for stage in SimSet::ALL {
            schedule.add_systems(
                (Refresh::give_parts, Refresh::run)
                    .chain()
                    .in_set(SimEdge::After(stage)),
            );
        }
    }

    /// Ends the applications of the modifiers `carrier` holds that grant `tag`: an instance no
    /// hold keeps ends, and one a passive, an aura, an area or a player holds stays, as its holder
    /// would apply it again.
    fn purge(world: &mut World, carrier: StableId, tag: Tag) {
        let Some(entity) = world.entity_of(carrier) else {
            return;
        };
        let book = world.resource::<ModifierBook>().clone();
        if let Some(mut carried) = CarriedMut::of(world, entity) {
            carried.purge(|id| book.tags(id).contains(tag));
        }
    }

    fn add_modifier(
        world: &mut World,
        target: StableId,
        id: ModifierId,
        applier: Applier,
        duration: Option<Ticks>,
    ) {
        let Some(entity) = world.entity_of(target) else {
            return;
        };
        if world.entity(entity).contains::<Dead>() {
            return;
        }
        let now = world.resource::<SimTick>().start();
        let rate = *world.resource::<TickRate>();
        if !world.contains_resource::<StatBook>() {
            return;
        }
        let book = world.resource::<ModifierBook>();
        let source = applier
            .source
            .and_then(|source| ParamSource::of(world, source));
        let params = world.resource::<ParamBook>();
        let param = |place: &ParamPlace| {
            let (action, rank) = (applier.action, applier.rank);
            params.modifier_param(id, action, rank, place, source.as_ref())
        };
        let application = book.application(id, applier, duration, now, rate, param);
        if let Some(mut carried) = CarriedMut::of(world, entity) {
            carried.apply(application);
        }
    }

    fn remove_modifier(
        world: &mut World,
        carrier: StableId,
        id: ModifierId,
        source: Option<StableId>,
    ) {
        let entity = world.entity_of(carrier);
        if let Some(mut carried) = entity.and_then(|entity| CarriedMut::of(world, entity)) {
            carried.remove(id, source);
        }
    }

    /// Writes what a call wrote to `handle`, its stacks and state, to its instance, if it still
    /// holds.
    pub(super) fn write_handle(world: &mut World, handle: &ModifierHandle) {
        let data = handle.data();
        if !data.written {
            return;
        }
        let now = world.resource::<SimTick>().start();
        let entity = world.entity_of(data.carrier);
        if let Some(mut carried) = entity.and_then(|entity| CarriedMut::of(world, entity)) {
            carried.write(data.id, data.source, data.stacks, &data.state, now);
        }
    }

    /// Gives the match the mode's stat book and pool book, whose names its scripts read.
    pub(crate) fn load_books(world: &mut World, book: StatBook, pools: PoolBook) {
        let view = world.non_send::<View>();
        StatsColumn::share_stat_names(view, book.names());
        StatsColumn::share_pool_names(view, pools.names());
        world.insert_resource(book);
        world.insert_resource(pools);
    }

    /// `has_way` of the match's books at its rate.
    pub(crate) fn modifier_has_way(
        world: &World,
        id: ModifierId,
        action: Option<ActionId>,
        rank: Rank,
    ) -> bool {
        let (book, params) = (
            world.resource::<ModifierBook>(),
            world.resource::<ParamBook>(),
        );
        book.has_way(id, action, rank, params, *world.resource::<TickRate>())
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_sim::{SimWorld, StableId};

    use crate::stats::Stats;
    use crate::stats::applier::Applier;
    use crate::stats::lifetime::Hold;
    use crate::stats::modifier_book::ModifierBook;
    use crate::stats::modifiers::Modifiers;
    use crate::stats::stats_effect::StatsEffect;
    use crate::units::action_id::ActionId;
    use crate::units::modifier_id::ModifierId;
    use crate::values::rank::Rank;

    impl Stats {
        /// The modifier `name` of `package`, as the match loaded it.
        pub fn modifier(world: &World, package: u16, name: &str) -> Option<ModifierId> {
            world.resource::<ModifierBook>().named(package, name)
        }

        /// Gives `target` the modifier `id` from `source`, by `action` at `rank`, and a passive when
        /// `passive`, as an application in the running tick would.
        pub fn give_modifier(
            world: &mut World,
            target: StableId,
            id: ModifierId,
            from: Option<(StableId, Option<ActionId>, u8)>,
            passive: bool,
        ) {
            let (source, action, rank) = from.map_or((None, None, 1), |(source, action, rank)| {
                (Some(source), action, rank)
            });
            let rank = Rank::new(rank).expect("a modifier is given at a rank");
            let applier = Applier {
                source,
                action,
                rank,
                hold: passive.then_some(Hold::Passive),
            };
            let add = StatsEffect::Add {
                target,
                id,
                duration: None,
            };
            add.apply_by(world, applier);
        }

        /// The modifiers `id` carries, each with its source, in their order.
        pub fn carried_modifiers(
            world: &World,
            id: StableId,
        ) -> Vec<(ModifierId, Option<StableId>)> {
            let entity = world.entity_of(id).expect("a unit of the match");
            world
                .get::<Modifiers>(entity)
                .into_iter()
                .flat_map(Modifiers::iter)
                .map(|instance| (instance.id, instance.source))
                .collect()
        }
    }
}

#[cfg(test)]
mod tests;
