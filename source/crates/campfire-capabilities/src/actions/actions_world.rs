#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {}

#[cfg(test)]
pub(crate) mod test_world {
    use bevy_ecs::world::{Mut, World};
    use campfire_common::Ticks;
    use campfire_math::Num;
    use campfire_script::ScriptId;
    use campfire_sim::TickRate;

    use crate::actions::Actions;
    use crate::actions::action::{Action, Aim};
    use crate::actions::action_book::ActionBook;
    use crate::actions::action_data::ActionData;
    use crate::actions::action_names::ActionNames;
    use crate::actions::action_parts::ActionParts;
    use crate::actions::action_range::ActionRange;
    use crate::actions::actions_column::ActionsColumn;
    use crate::actions::cost_target::CostTarget;
    use crate::actions::effect_lists::EffectLists;
    use crate::actions::effect_names::EffectNames;
    use crate::actions::kind_spec::KindSpec;
    use crate::actions::rank_values::RankValues;
    use crate::players::resource_amount::ResourceAmount;
    use crate::progression::progression_column::ProgressionColumn;
    use crate::projectiles::projectile_spec::ProjectileSpec;
    use crate::scripts::hook_set::HookSet;
    use crate::scripts::script_book::ScriptBook;
    use crate::stats::Stats;
    use crate::stats::modifier_book::ModifierBook;
    use crate::stats::param_book::ParamBook;
    use crate::stats::pool_cost::PoolCost;
    use crate::stats::pool_id::PoolId;
    use crate::stats::stat_id::StatId;
    use crate::stats::stats_column::StatsColumn;
    use crate::stats::stats_world::test_world::StatsWorldTest;
    use crate::units::action_id::ActionId;
    use crate::units::by_type::ByType;
    use crate::units::filter::Filter;
    use crate::units::modifier_id::ModifierId;
    use crate::units::tag::Tag;
    use crate::units::track_id::TrackId;
    use crate::units::type_scope::TypeScope;
    use crate::units::unit_type::UnitType;
    use crate::units::view::View;
    use crate::values::damage_kind::DamageKind;
    use crate::values::declared_name::DeclaredName;
    use crate::values::error::TimeTooLarge;
    use crate::values::filter_data::FilterData;
    use crate::values::param::Param;
    use crate::values::stat::Stat;

    /// What the actions module gives a world for its own tests.
    pub(crate) trait ActionsWorldTest {
        /// Loads the effect lists of `action` of `package`, which loaded last from `data`, which
        /// the package load checked: each name resolved to its id, each param to its place among
        /// the action's params.
        fn load_effect_lists(&mut self, action: ActionId, package: u16, data: &ActionData);

        /// Loads the action `name` of `package`, of `ranks` ranks, into the match, which the
        /// package load checked, with its compiled script exactly when its data names one: its
        /// capability fields at each rank, times in milliseconds as ticks at the match's rate,
        /// rounded up.
        fn load_action(
            &mut self,
            package: u16,
            name: &str,
            data: &ActionData,
            script: Option<ScriptId>,
            ranks: u8,
        ) -> Result<ActionId, TimeTooLarge>;

        /// Adds a train of `unit_type` that takes `time` and costs `resource_cost`, no pool and no
        /// cooldown, to the match's action book.
        fn add_train(
            &mut self,
            unit_type: UnitType,
            time: Ticks,
            resource_cost: Option<ResourceAmount>,
        ) -> ActionId;

        /// Adds a build of `unit_type` within `range` meters, aimed at a point, that takes `time` and costs
        /// `resource_cost`, no pool and no cooldown, to the match's action book.
        fn add_build(
            &mut self,
            unit_type: UnitType,
            range: Num,
            time: Ticks,
            resource_cost: Option<ResourceAmount>,
        ) -> ActionId;
    }

    impl ActionsWorldTest for World {
        fn load_effect_lists(&mut self, action: ActionId, package: u16, data: &ActionData) {
            let view = self.non_send::<View>().clone();
            let mut lists = self.remove_resource::<EffectLists>().unwrap();
            let names = MatchEffectNames {
                world: self,
                view: &view,
                action,
                package,
            };
            lists.push(action, data, &names);
            self.insert_resource(lists);
        }

        fn load_action(
            &mut self,
            package: u16,
            name: &str,
            data: &ActionData,
            script: Option<ScriptId>,
            ranks: u8,
        ) -> Result<ActionId, TimeTooLarge> {
            let rate = *self.resource::<TickRate>();
            let view = self.non_send::<View>().clone();
            let names = MatchNames {
                world: self,
                view: &view,
                modifiers: self.resource::<ModifierBook>(),
            };
            let parts = ActionParts::of(data, package, ranks, rate, &names)?;
            let id = self.resource_scope(|world, mut actions: Mut<'_, ActionBook>| {
                let scripts = world.resource::<ScriptBook>();
                actions.load(scripts, package, name, data, script, parts)
            });
            let places = self.stat_places(data.params.values().flat_map(Param::stats));
            self.load_action_params(id, &data.params, |stat| places[stat]);
            let book = self.resource::<ActionBook>().clone();
            ActionsColumn::share(self.non_send::<View>(), book);
            Ok(id)
        }

        fn add_train(
            &mut self,
            unit_type: UnitType,
            time: Ticks,
            resource_cost: Option<ResourceAmount>,
        ) -> ActionId {
            Actions::push(
                self,
                Action {
                    package: 0,
                    name: "train".into(),
                    kind: KindSpec::Train(unit_type),
                    passive: None,
                    hold: None,
                    aim: Aim::None,
                    ranks: vec![RankValues {
                        range: ActionRange::Global,
                        cooldown: Ticks::ZERO,
                        cost: PoolCost::default(),
                        windup: time,
                        charges: None,
                        toggle: None,
                        channel: None,
                        charge: None,
                    }],
                    resource_costs: resource_cost.into_iter().collect(),
                    script: None,
                    hooks: HookSet::default(),
                    delivery: None,
                },
            )
        }

        fn add_build(
            &mut self,
            unit_type: UnitType,
            range: Num,
            time: Ticks,
            resource_cost: Option<ResourceAmount>,
        ) -> ActionId {
            Actions::push(
                self,
                Action {
                    package: 0,
                    name: "build".into(),
                    kind: KindSpec::Build(unit_type),
                    passive: None,
                    hold: None,
                    aim: Aim::Point { clamp: false },
                    ranks: vec![RankValues {
                        range: ActionRange::Meters(range),
                        cooldown: Ticks::ZERO,
                        cost: PoolCost::default(),
                        windup: time,
                        charges: None,
                        toggle: None,
                        channel: None,
                        charge: None,
                    }],
                    resource_costs: resource_cost.into_iter().collect(),
                    script: None,
                    hooks: HookSet::default(),
                    delivery: None,
                },
            )
        }
    }

    /// The names of an action's effect lists as a match's world resolves them: its view, its param
    /// book, and the modifiers of the action's package.
    #[derive(Debug)]
    struct MatchEffectNames<'w> {
        world: &'w World,
        view: &'w View,
        action: ActionId,
        package: u16,
    }

    impl EffectNames for MatchEffectNames<'_> {
        fn param(&self, name: &DeclaredName) -> usize {
            self.world
                .resource::<ParamBook>()
                .actions()
                .named(self.action.index(), name.as_str())
                .expect("the load checked an effect's param")
        }

        fn damage_kind(&self, name: &DeclaredName) -> DamageKind {
            self.view
                .damage_kind_named(name.as_str())
                .expect("the load checked an effect's damage kind")
        }

        fn pool(&self, name: &DeclaredName) -> PoolId {
            StatsColumn::pool_id_named(self.view, name.as_str())
                .expect("the load checked an effect's pool")
        }

        fn modifier(&self, name: &DeclaredName) -> ModifierId {
            Stats::modifier(self.world, self.package, name.as_str())
                .expect("the load checked an effect's modifier")
        }

        fn track(&self, name: &DeclaredName) -> TrackId {
            ProgressionColumn::track_named(self.view, name.as_str())
                .expect("the load checked an effect's track")
        }

        fn tag(&self, name: &DeclaredName) -> Tag {
            self.view
                .tag_named(name.as_str())
                .expect("the load checked an effect's tag")
        }

        fn package_type(&self, name: &DeclaredName) -> UnitType {
            let scope = TypeScope::of_package(self.package);
            self.view
                .types_mut()
                .named(scope, name.as_str())
                .expect("the load checked an effect's unit type")
        }
    }

    /// The names of an action's data as a match's world resolves them: its view, its book of
    /// modifiers, and the world's projectile specs.
    #[derive(Debug)]
    struct MatchNames<'w> {
        world: &'w World,
        view: &'w View,
        modifiers: &'w ModifierBook,
    }

    impl ActionNames for MatchNames<'_> {
        fn stat(&self, stat: &Stat) -> StatId {
            self.view
                .stat_index(stat)
                .expect("the load checked the stats")
        }

        fn damage_kind(&self, name: &DeclaredName) -> DamageKind {
            self.view
                .damage_kind_named(name.as_str())
                .expect("the load checked the damage kind")
        }

        fn cost_target(&self, name: &DeclaredName) -> Option<CostTarget> {
            self.view.cost_target(name.as_str())
        }

        fn filter(&self, filter: &FilterData) -> Filter {
            self.view
                .resolve_filter(filter)
                .expect("the load checked the filter's tags")
        }

        fn modifier(&self, package: u16, name: &DeclaredName) -> ModifierId {
            self.modifiers
                .named(package, name.as_str())
                .expect("the load checked the modifier")
        }

        fn homes(&self, package: u16, name: &DeclaredName) -> bool {
            let unit_type = self.unit_type(package, name);
            let specs = self.world.resource::<ByType<ProjectileSpec>>();
            specs
                .get(unit_type)
                .expect("a test loads a projectile type's spec before its action")
                .homing
        }

        fn unit_type(&self, package: u16, name: &DeclaredName) -> UnitType {
            let scope = TypeScope::of_package(package);
            self.view
                .types_mut()
                .named(scope, name.as_str())
                .expect("a test loads an action's unit type before the action")
        }
    }
}
