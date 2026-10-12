use bevy_ecs::world::{Mut, World};
use campfire_common::PlayerSlot;

use crate::players::players_world::PlayersWorld;
use crate::stats::player_modifiers::PlayerModifiers;

/// What the stats module gives a world, for its own crate.
pub(crate) trait StatsWorldCrate {
    /// The modifiers player `slot` holds, to change; none before its entity spawns.
    fn player_modifiers_mut(&mut self, slot: PlayerSlot) -> Option<Mut<'_, PlayerModifiers>>;
}

impl StatsWorldCrate for World {
    fn player_modifiers_mut(&mut self, slot: PlayerSlot) -> Option<Mut<'_, PlayerModifiers>> {
        let entity = self.player_entity(slot)?;
        self.get_mut(entity)
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
