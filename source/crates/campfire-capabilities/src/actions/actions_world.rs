use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::TickRate;

use crate::actions::action_target::ActionTarget;
use crate::actions::actions_effect::ActionsEffect;
use crate::actions::amount::Amount;
use crate::actions::does::Does;
use crate::actions::effect_data::EffectTo;
use crate::actions::effect_lists::EffectLists;
use crate::actions::effect_queues::EffectQueues;
use crate::actions::lists_of::ListsOf;
use crate::scripts::error::CallError;
use crate::scripts::frame::Frame;
use crate::scripts::hook::Hook;
use crate::stats::stats_effect::StatsEffect;
use crate::units::spawn_at::SpawnAt;
use crate::units::view::View;

/// What the actions module gives a world, for its own crate.
pub(crate) trait ActionsWorldCrate {
    /// Queues the list of `of` that runs before its `hook` in `frame`, a call of its action at
    /// its rank, which the match runs: each effect to the unit `reached`, what the list reached, or
    /// to the acting unit; a spawn where that unit stands, or at the point `reached` is. A
    /// modifier, a purge and a spawn queue here, as `stats` and the core are below the action
    /// pipeline; every other effect queues as its capability registered, which can fail the
    /// call, as experience to a unit without the track fails `ctx.add_xp`.
    fn queue_effect_list(
        &self,
        of: ListsOf,
        hook: Hook,
        frame: &mut Frame,
        view: &View,
        reached: ActionTarget,
    ) -> Result<(), CallError>;
}

impl ActionsWorldCrate for World {
    fn queue_effect_list(
        &self,
        of: ListsOf,
        hook: Hook,
        frame: &mut Frame,
        view: &View,
        reached: ActionTarget,
    ) -> Result<(), CallError> {
        let list = self.resource::<EffectLists>().of(of, hook);
        if list.is_empty() {
            return Ok(());
        }
        let rate = *self.resource::<TickRate>();
        let queues = self.resource::<EffectQueues>();
        let acting = frame.acting();
        let reached_unit = reached.unit();
        let duration = |ms: Amount, frame: &Frame| {
            // Whole, as the load checked, so the floor is exact.
            let ms = ms.number(frame).floor();
            let ms = u64::try_from(ms).expect("the load checked a duration not negative");
            rate.duration(ms)
                .expect("the load checked a duration within reach")
        };
        for &listed in list {
            let unit = || match listed.to {
                EffectTo::Reached => {
                    reached_unit.expect("the load lets only an effect to the source reach no unit")
                }
                EffectTo::Source => acting.expect("an action's list runs for its acting unit"),
            };
            match listed.does {
                Does::Spawn {
                    unit_type,
                    duration_ms,
                } => {
                    let source = acting.and_then(|id| view.row(id));
                    let source = source.expect("an action's list runs for its acting unit");
                    let pos = match (listed.to, reached) {
                        (EffectTo::Source, _) => source.pos,
                        (EffectTo::Reached, ActionTarget::Point(at)) => at,
                        (EffectTo::Reached, ActionTarget::Unit(id)) => {
                            view.row(id)
                                .expect("a list reaches a unit the view holds")
                                .pos
                        }
                        (EffectTo::Reached, ActionTarget::None) => {
                            unreachable!(
                                "the load lets a spawn reach no place only from the source"
                            )
                        }
                    };
                    let at = SpawnAt {
                        id: frame.take_id(),
                        unit_type,
                        team: source.team,
                        pos,
                        angle: Num::ZERO,
                    };
                    let life = duration_ms.map(|ms| duration(ms, frame));
                    frame.effects.push(ActionsEffect {
                        at,
                        owner: source.owner,
                        life,
                    });
                }
                Does::Modifier { id, duration_ms } => {
                    let duration = duration_ms.map(|ms| duration(ms, frame));
                    frame.effects.push(StatsEffect::Add {
                        target: unit(),
                        id,
                        duration,
                    });
                }
                Does::Purge { tag } => frame.effects.push(StatsEffect::Purge {
                    carrier: unit(),
                    tag,
                }),
                Does::Capability(does) => {
                    queues.of(does.capability())(does, unit(), reached_unit, frame, view)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use std::num::NonZeroU8;

    use bevy_ecs::world::World;
    use campfire_common::Ticks;
    use campfire_math::Num;

    use crate::actions::action::{Action, Aim};
    use crate::actions::action_book::ActionBook;
    use crate::actions::action_book::internals::TestWeapon;
    #[cfg(any(test, feature = "bench"))]
    use crate::actions::action_range::ActionRange;
    use crate::actions::actions_column::ActionsColumn;
    use crate::actions::delivery::{Delivery, DeliveryShape};
    use crate::actions::fan::Fan;
    #[cfg(any(test, feature = "bench"))]
    use crate::actions::gather_spec::GatherSpec;
    use crate::actions::kind_spec::KindSpec;
    use crate::actions::rank_values::RankValues;
    use crate::actions::weapon::Weapon;
    use crate::scripts::hook_set::HookSet;
    #[cfg(any(test, feature = "bench"))]
    use crate::stats::pool_cost::PoolCost;
    use crate::units::action_id::ActionId;
    #[cfg(any(test, feature = "bench"))]
    use crate::units::filter::Filter;
    use crate::units::view::View;
    use crate::values::damage_kind::DamageKind;

    /// What the actions module gives a world for the tests and benches of every crate.
    pub trait ActionsWorldInternals {
        /// The action `name` of `package`, as the match loaded it.
        fn action_named(&self, package: u16, name: &str) -> Option<ActionId>;
    }

    impl ActionsWorldInternals for World {
        fn action_named(&self, package: u16, name: &str) -> Option<ActionId> {
            self.resource::<ActionBook>().named(package, name)
        }
    }

    /// What the actions module gives a world for the tests and benches of its own crate.
    pub(crate) trait ActionsWorldCrateInternals {
        /// Adds `weapon` to the match's action book, which deals damage of the first kind.
        fn add_weapon(&mut self, weapon: TestWeapon) -> ActionId;

        /// Adds a gather of `spec` from the nodes `aim` selects, within `range` meters, a trip of
        /// `time`, to the match's action book.
        #[cfg(any(test, feature = "bench"))]
        fn add_gather(
            &mut self,
            spec: GatherSpec,
            aim: Filter,
            range: Num,
            time: Ticks,
        ) -> ActionId;

        /// Adds `action` to the match's action book, and shares the book with the script view.
        fn push_action(&mut self, action: Action) -> ActionId;
    }

    impl ActionsWorldCrateInternals for World {
        fn add_weapon(&mut self, weapon: TestWeapon) -> ActionId {
            let delivery = weapon.projectile.map(|unit_type| Delivery {
                unit_type,
                shape: DeliveryShape::Projectile {
                    fan: Fan {
                        count: NonZeroU8::MIN,
                        spread_deg: Num::ZERO,
                    },
                    homes: true,
                },
            });
            self.push_action(Action {
                package: 0,
                name: "weapon".into(),
                kind: KindSpec::Attack(Weapon {
                    rate: weapon.rate,
                    damage: weapon.damage,
                    kind: DamageKind::new(0),
                }),
                passive: None,
                hold: None,
                aim: Aim::Unit(weapon.aim),
                ranks: vec![RankValues {
                    range: weapon.range,
                    cooldown: Ticks::ZERO,
                    cost: weapon.cost,
                    windup: weapon.windup,
                    charges: None,
                    toggle: None,
                    channel: None,
                    charge: None,
                }],
                resource_costs: weapon.resource_cost.into_iter().collect(),
                script: None,
                hooks: HookSet::default(),
                delivery,
            })
        }

        #[cfg(any(test, feature = "bench"))]
        fn add_gather(
            &mut self,
            spec: GatherSpec,
            aim: Filter,
            range: Num,
            time: Ticks,
        ) -> ActionId {
            self.push_action(Action {
                package: 0,
                name: "gather".into(),
                kind: KindSpec::Gather(spec),
                passive: None,
                hold: None,
                aim: Aim::Unit(aim),
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
                resource_costs: Box::default(),
                script: None,
                hooks: HookSet::default(),
                delivery: None,
            })
        }

        fn push_action(&mut self, action: Action) -> ActionId {
            let mut book = self.resource_mut::<ActionBook>();
            let id = book.push(action);
            let book = book.clone();
            ActionsColumn::share(self.non_send::<View>(), book);
            id
        }
    }
}

#[cfg(test)]
pub(crate) mod test_world {
    use bevy_ecs::world::{Mut, World};
    use campfire_common::Ticks;
    use campfire_math::Num;
    use campfire_script::ScriptId;
    use campfire_sim::TickRate;

    use crate::actions::action::{Action, Aim};
    use crate::actions::action_book::ActionBook;
    use crate::actions::action_data::ActionData;
    use crate::actions::action_names::ActionNames;
    use crate::actions::action_parts::ActionParts;
    use crate::actions::action_range::ActionRange;
    use crate::actions::actions_column::ActionsColumn;
    use crate::actions::actions_world::internals::ActionsWorldCrateInternals;
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
    use crate::stats::modifier_book::ModifierBook;
    use crate::stats::param_book::ParamBook;
    use crate::stats::pool_cost::PoolCost;
    use crate::stats::pool_id::PoolId;
    use crate::stats::stat_id::StatId;
    use crate::stats::stats_column::StatsColumn;
    use crate::stats::stats_world::internals::StatsWorldInternals;
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
            self.push_action(Action {
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
            })
        }

        fn add_build(
            &mut self,
            unit_type: UnitType,
            range: Num,
            time: Ticks,
            resource_cost: Option<ResourceAmount>,
        ) -> ActionId {
            self.push_action(Action {
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
            })
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
            self.world
                .modifier_named(self.package, name.as_str())
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
