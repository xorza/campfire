use bevy_ecs::entity::Entity;
use bevy_ecs::world::{Mut, World};
use campfire_common::PlayerSlot;
use campfire_sim::{SimWorld, StableId, TickRate};

use crate::players::players_world::PlayersWorld;
use crate::stats::carried_mut::CarriedMut;
use crate::stats::level::Level;
use crate::stats::life_pool::LifePool;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::modifier_clocks::ModifierClocks;
use crate::stats::modifiers::Modifiers;
use crate::stats::param_book::ParamBook;
use crate::stats::param_source::ParamSource;
use crate::stats::player_modifiers::PlayerModifiers;
use crate::stats::pool_book::PoolBook;
use crate::stats::stat_book::StatBook;
use crate::stats::stats_column::StatsColumn;
use crate::stats::unit_pools::PoolMeters;
use crate::stats::unit_stats::UnitStats;
use crate::units::action_id::ActionId;
use crate::units::modifier_id::ModifierId;
use crate::units::unit_type::UnitType;
use crate::units::view::View;
use crate::values::rank::Rank;

/// What the stats module gives a world, for every crate.
pub trait StatsWorld {
    /// The pools of the unit of `entity`, the life pool the match's.
    fn pool_meters(&self, entity: Entity) -> PoolMeters<'_>;
}

impl StatsWorld for World {
    fn pool_meters(&self, entity: Entity) -> PoolMeters<'_> {
        let life_pool = self.get_resource::<LifePool>().map(|life| life.0);
        PoolMeters::new(life_pool, self.get(entity), self.get(entity))
    }
}

/// What the stats module gives a world, for its own crate.
pub(crate) trait StatsWorldCrate {
    /// The modifiers and clocks of `entity`; none when it carries none.
    fn carried_mut(&mut self, entity: Entity) -> Option<CarriedMut<'_>>;

    /// `has_way` of the match's books at its rate.
    fn modifier_has_way(&self, id: ModifierId, action: Option<ActionId>, rank: Rank) -> bool;

    /// Gives the match the mode's stat book and pool book, whose names its scripts read.
    fn load_stat_books(&mut self, book: StatBook, pools: PoolBook);

    /// `id` as a source; `None` when it is gone, or the match has no stat book.
    fn param_source(&self, id: StableId) -> Option<ParamSource<'_>>;

    /// The modifiers player `slot` holds, to change; none before its entity spawns.
    fn player_modifiers_mut(&mut self, slot: PlayerSlot) -> Option<Mut<'_, PlayerModifiers>>;
}

impl StatsWorldCrate for World {
    fn carried_mut(&mut self, entity: Entity) -> Option<CarriedMut<'_>> {
        let (modifiers, clocks) = self
            .get_entity_mut(entity)
            .ok()?
            .into_components_mut::<(&mut Modifiers, &mut ModifierClocks)>()
            .ok()?;
        Some(CarriedMut::new(modifiers, clocks))
    }

    fn modifier_has_way(&self, id: ModifierId, action: Option<ActionId>, rank: Rank) -> bool {
        let (book, params) = (
            self.resource::<ModifierBook>(),
            self.resource::<ParamBook>(),
        );
        book.has_way(id, action, rank, params, *self.resource::<TickRate>())
    }

    fn load_stat_books(&mut self, book: StatBook, pools: PoolBook) {
        let view = self.non_send::<View>();
        StatsColumn::share_stat_names(view, book.names());
        StatsColumn::share_pool_names(view, pools.names());
        self.insert_resource(book);
        self.insert_resource(pools);
    }

    fn param_source(&self, id: StableId) -> Option<ParamSource<'_>> {
        let book = self.get_resource::<StatBook>()?;
        let entity = self.entity_of(id)?;
        let id = self.entity(entity);
        Some(ParamSource::of_parts(
            book,
            *id.get::<UnitType>()?,
            id.get::<Level>(),
            id.get::<UnitStats>(),
        ))
    }

    fn player_modifiers_mut(&mut self, slot: PlayerSlot) -> Option<Mut<'_, PlayerModifiers>> {
        let entity = self.player_entity(slot)?;
        self.get_mut(entity)
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_sim::{SimWorld, StableId};

    use crate::stats::applier::Applier;
    use crate::stats::lifetime::Hold;
    use crate::stats::modifier_book::ModifierBook;
    use crate::stats::modifiers::Modifiers;
    use crate::stats::stats_effect::StatsEffect;
    use crate::units::action_id::ActionId;
    use crate::units::modifier_id::ModifierId;
    use crate::values::rank::Rank;

    /// What the stats module gives a world for the tests and benches of every crate.
    pub trait StatsWorldInternals {
        /// The modifier `name` of `package`, as the match loaded it.
        fn modifier_named(&self, package: u16, name: &str) -> Option<ModifierId>;

        /// Gives `target` the modifier `id` from `source`, by `action` at `rank`, and a passive when
        /// `passive`, as an application in the running tick would.
        fn give_modifier(
            &mut self,
            target: StableId,
            id: ModifierId,
            from: Option<(StableId, Option<ActionId>, u8)>,
            passive: bool,
        );

        /// The modifiers `id` carries, each with its source, in their order.
        fn carried_modifiers(&self, id: StableId) -> Vec<(ModifierId, Option<StableId>)>;
    }

    impl StatsWorldInternals for World {
        fn modifier_named(&self, package: u16, name: &str) -> Option<ModifierId> {
            self.resource::<ModifierBook>().named(package, name)
        }

        fn give_modifier(
            &mut self,
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
            add.apply_by(self, applier);
        }

        fn carried_modifiers(&self, id: StableId) -> Vec<(ModifierId, Option<StableId>)> {
            let entity = self.entity_of(id).expect("a unit of the match");
            self.get::<Modifiers>(entity)
                .into_iter()
                .flat_map(Modifiers::iter)
                .map(|instance| (instance.id, instance.source))
                .collect()
        }
    }
}

#[cfg(test)]
pub(crate) mod test_world {
    use std::collections::BTreeMap;

    use bevy_ecs::world::{Mut, World};
    use campfire_math::Num;
    use campfire_script::ScriptId;
    use campfire_sim::TickRate;

    use crate::scripts::ctx::Ctx;
    use crate::scripts::script_book::ScriptBook;
    use crate::stats::modifier_book::{ModifierBook, ModifierLoad, PackageModifier};
    use crate::stats::modifier_data::ModifierData;
    use crate::stats::param_book::ParamBook;
    use crate::stats::pool_book::PoolBook;
    use crate::stats::stat_book::StatBook;
    use crate::stats::stat_id::StatId;
    use crate::stats::stat_op::StatOp;
    use crate::stats::stat_rule::StatRule;
    use crate::stats::stats_call::StatsCall;
    use crate::stats::stats_column::StatsColumn;
    use crate::units::action_id::ActionId;
    use crate::units::modifier_id::ModifierId;
    use crate::units::tag_set::TagSet;
    use crate::units::view::View;
    use crate::values::declared_name::DeclaredName;
    use crate::values::param::Param;
    use crate::values::stat::Stat;

    /// What the stats module gives a world for its own tests.
    pub(crate) trait StatsWorldTest {
        /// Adds to the match's books a modifier of no params, script, times or state that changes
        /// each stat of `changes` by its op, by a value its instances hold, and grants `tags`:
        /// what a test's instances are instances of. Its id, after every modifier before it.
        fn push_modifier_changes(
            &mut self,
            changes: &[(StatId, StatOp)],
            tags: TagSet,
        ) -> ModifierId;

        /// Loads `data` as the modifier `name` of `package`: 0 the mode, then each package it
        /// depends on, in its manifest's order, with its compiled `script` exactly when its data
        /// names one. Modifiers load by package, then name.
        fn load_modifier(
            &mut self,
            package: u16,
            name: &str,
            data: &ModifierData,
            script: Option<ScriptId>,
        );

        /// Gives a match with no mode the stat book `book`, with no pool stats; the pools its
        /// scripts name stay named.
        fn load_stat_book(&mut self, book: StatBook);

        /// Gives a match with no mode the stat book of `rules`, with no unit type and no pool stats;
        /// the pools its scripts name stay named.
        fn load_stat_rules(&mut self, rules: &BTreeMap<Stat, StatRule>);

        /// Adds the params of `action`, the one the match's action book loaded last, each stat at
        /// its place `stat` gives.
        fn load_action_params(
            &mut self,
            action: ActionId,
            params: &BTreeMap<DeclaredName, Param>,
            stat: impl Fn(&Stat) -> StatId,
        );

        /// Adds the params of `modifier`, the one the match's modifier book loaded last, each stat
        /// at its place `stat` gives.
        fn load_modifier_params(
            &mut self,
            modifier: ModifierId,
            params: &BTreeMap<DeclaredName, Param>,
            stat: impl Fn(&Stat) -> StatId,
        );

        /// The place of each of `stats` in the match's book, which a world that names no stat
        /// need not have.
        fn stat_places<'s>(
            &self,
            stats: impl IntoIterator<Item = &'s Stat>,
        ) -> BTreeMap<&'s Stat, StatId>;
    }

    impl StatsWorldTest for World {
        fn push_modifier_changes(
            &mut self,
            changes: &[(StatId, StatOp)],
            tags: TagSet,
        ) -> ModifierId {
            let id = self
                .resource_mut::<ModifierBook>()
                .push_changes(changes, tags);
            self.load_modifier_params(id, &BTreeMap::new(), |_| StatId::new(0));
            id
        }

        fn load_modifier(
            &mut self,
            package: u16,
            name: &str,
            data: &ModifierData,
            script: Option<ScriptId>,
        ) {
            let name = DeclaredName::new(name).expect("a modifier's name is a name");
            self.resource_scope(|world, mut book: Mut<'_, ModifierBook>| {
                let scripts = world.resource::<ScriptBook>();
                let view = world.non_send::<View>();
                let places = world.stat_places(data.stats.keys());
                let mut types = view.types_mut();
                let load = ModifierLoad {
                    scripts,
                    types: &mut types,
                    stat: |stat: &Stat| places[stat],
                    rate: *world.resource::<TickRate>(),
                };
                let modifier = PackageModifier {
                    name: &name,
                    data,
                    script,
                };
                book.load(load, package, &[modifier])
                    .expect("a test's modifier loads");
            });
            let book = self.resource::<ModifierBook>();
            let id = book
                .named(package, name.as_str())
                .expect("the modifier loaded");
            StatsColumn::share_modifiers(self.non_send::<View>(), book.clone());
            let places = self.stat_places(data.params.values().flat_map(Param::stats));
            self.load_modifier_params(id, &data.params, |stat| places[stat]);
        }

        fn load_stat_book(&mut self, book: StatBook) {
            StatsColumn::share_stat_names(self.non_send::<View>(), book.names());
            self.insert_resource(book);
            self.insert_resource(PoolBook::default());
        }

        fn load_stat_rules(&mut self, rules: &BTreeMap<Stat, StatRule>) {
            self.load_stat_book(StatBook::new(rules, [], Num::MAX));
        }

        fn load_action_params(
            &mut self,
            action: ActionId,
            params: &BTreeMap<DeclaredName, Param>,
            stat: impl Fn(&Stat) -> StatId,
        ) {
            let run = self.resource_mut::<ParamBook>().push_action(params, stat);
            assert_eq!(run, action.index(), "one run of params per action");
            share_params(self);
        }

        fn load_modifier_params(
            &mut self,
            modifier: ModifierId,
            params: &BTreeMap<DeclaredName, Param>,
            stat: impl Fn(&Stat) -> StatId,
        ) {
            let run = self.resource_mut::<ParamBook>().push_modifier(params, stat);
            assert_eq!(run, modifier.index(), "one run of params per modifier");
            share_params(self);
        }

        fn stat_places<'s>(
            &self,
            stats: impl IntoIterator<Item = &'s Stat>,
        ) -> BTreeMap<&'s Stat, StatId> {
            stats
                .into_iter()
                .map(|stat| {
                    let book = self.resource::<StatBook>();
                    (stat, book.named(stat).expect("the load checked the stats"))
                })
                .collect()
        }
    }

    /// Gives the match's script frame, when it has one, the param book as it is now.
    fn share_params(world: &World) {
        if let Some(ctx) = world.get_non_send::<Ctx>() {
            let params = world.resource::<ParamBook>().clone();
            StatsCall::share_params(&mut ctx.frame(), params);
        }
    }
}
