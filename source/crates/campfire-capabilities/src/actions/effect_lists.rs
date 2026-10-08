use std::ops::Range;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::TickRate;
use serde::{Deserialize, Serialize};

use crate::actions::action_data::ActionData;
use crate::actions::action_target::ActionTarget;
use crate::actions::actions_effect::ActionsEffect;
use crate::actions::capability_does::CapabilityDoes;
use crate::actions::effect_data::{
    DamageFields, EffectData, EffectTo, Effecting, HealFields, LaunchFields, ModifierFields,
    MoveData, PurgeFields, RestoreFields, SpawnFields, XpFields,
};
use crate::actions::effect_names::EffectNames;
use crate::actions::effect_queues::EffectQueues;
use crate::scripts::error::CallError;
use crate::scripts::frame::Frame;
use crate::scripts::hook::Hook;
use crate::stats::stats_call::StatsCall;
use crate::stats::stats_effect::StatsEffect;
use crate::units::action_id::ActionId;
use crate::units::modifier_id::ModifierId;
use crate::units::script_view::View;
use crate::units::spawner::SpawnAt;
use crate::units::tag::Tag;
use crate::units::unit_type::UnitType;
use crate::values::number::Number;

/// The effect lists of each action, their names resolved as the action loaded: one buffer, by
/// action id the run of each of its lists, `on_resolve`, `on_hit` and `on_end`, and by launch id
/// the run of each list a launch holds, `on_hit` and `on_end`. An action past the end has none.
/// Package data, not state.
#[derive(Resource, Debug, Default)]
pub(crate) struct EffectLists {
    effects: Vec<Listed>,
    runs: Vec<[Range<u32>; 3]>,
    launches: Vec<[Range<u32>; 2]>,
}

/// A launch of an effect list, by its place among the match's launches, whose own lists its area
/// runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct LaunchId(u32);

/// Whose lists a call runs: an action's, or those a launch holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ListsOf {
    Action(ActionId),
    Launch(LaunchId),
}

/// An effect of a list: what it does, and to whom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Listed {
    pub(crate) does: Does,
    pub(crate) to: EffectTo,
}

/// What a listed effect does, its names resolved: an effect the action pipeline queues itself,
/// as `stats` and the core are below it, or one a capability above it queues.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Does {
    Modifier {
        id: ModifierId,
        duration_ms: Option<Amount>,
    },
    Purge {
        tag: Tag,
    },
    /// A unit of the mode's type `unit_type`, despawning `duration_ms` after it spawns when given.
    Spawn {
        unit_type: UnitType,
        duration_ms: Option<Amount>,
    },
    Capability(CapabilityDoes),
}

/// A number of a listed effect: a value, or the param at its place among its action's, which the
/// frame holds at the call's rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Amount {
    Value(Num),
    Param(usize),
}

impl EffectLists {
    /// Adds the lists of `action`, which loaded last, from `data`, which the package load
    /// checked, their names resolved by `names`: `on_resolve`, `on_hit` and `on_end`, and the
    /// lists each launch of them holds, however deep.
    pub(crate) fn push(&mut self, action: ActionId, data: &ActionData, names: &impl EffectNames) {
        if self.runs.len() <= action.index() {
            self.runs.resize(action.index() + 1, [0..0, 0..0, 0..0]);
        }
        let mut runs = [0..0, 0..0, 0..0];
        for (run, list) in runs
            .iter_mut()
            .zip([&data.on_resolve, &data.on_hit, &data.on_end])
        {
            *run = self.append(list, names);
        }
        self.runs[action.index()] = runs;
    }

    /// Appends `list`, its names resolved by `names`, after the lists its launches hold, so its
    /// own effects lie together; its run.
    fn append(&mut self, list: &[EffectData], names: &impl EffectNames) -> Range<u32> {
        let resolved: Vec<Listed> = list
            .iter()
            .map(|effect| Listed {
                does: self.resolve(&effect.does, names),
                to: effect.to,
            })
            .collect();
        let start = position(self.effects.len());
        self.effects.extend(resolved);
        start..position(self.effects.len())
    }

    /// What `does` does, its names resolved by `names`; a launch's lists appended first.
    fn resolve(&mut self, does: &Effecting, names: &impl EffectNames) -> Does {
        let amount = |number| Amount::of(number, names);
        let of_capability = match does {
            Effecting::Modifier(ModifierFields { id, duration_ms }) => {
                return Does::Modifier {
                    id: names.modifier(id),
                    duration_ms: duration_ms.as_ref().map(amount),
                };
            }
            Effecting::Purge(PurgeFields { tag }) => {
                return Does::Purge {
                    tag: names.tag(tag),
                };
            }
            Effecting::Spawn(SpawnFields {
                unit_type,
                duration_ms,
            }) => {
                return Does::Spawn {
                    unit_type: names.standing_type(unit_type),
                    duration_ms: duration_ms.as_ref().map(amount),
                };
            }
            Effecting::Damage(DamageFields {
                amount: dealt,
                kind,
            }) => CapabilityDoes::Damage {
                amount: amount(dealt),
                kind: names.damage_kind(kind),
            },
            Effecting::Heal(HealFields { amount: healed }) => CapabilityDoes::Heal {
                amount: amount(healed),
            },
            Effecting::Restore(RestoreFields {
                pool,
                amount: restored,
            }) => CapabilityDoes::Restore {
                pool: names.pool(pool),
                amount: amount(restored),
            },
            Effecting::Xp(XpFields {
                track,
                amount: given,
            }) => CapabilityDoes::Xp {
                track: names.track(track),
                amount: amount(given),
            },
            Effecting::Launch(LaunchFields {
                area,
                on_hit,
                on_end,
            }) => {
                let lists = [self.append(on_hit, names), self.append(on_end, names)];
                let launch = LaunchId(position(self.launches.len()));
                self.launches.push(lists);
                CapabilityDoes::Launch {
                    area: names.delivery_type(area),
                    launch,
                }
            }
            Effecting::Move(MoveData::Dash { to, speed }) => CapabilityDoes::Dash {
                to: *to,
                speed: amount(speed),
            },
            Effecting::Move(MoveData::KnockBack { from, distance, ms }) => {
                CapabilityDoes::KnockBack {
                    from: *from,
                    distance: amount(distance),
                    ms: amount(ms),
                }
            }
            Effecting::Planned(_) => unreachable!("the load refuses a planned effect"),
        };
        Does::Capability(of_capability)
    }

    /// The list of `of` that runs before its `hook`: an action's `on_resolve`, `on_hit` or
    /// `on_end`, or a launch's `on_hit` or `on_end`.
    pub(crate) fn of(&self, of: ListsOf, hook: Hook) -> &[Listed] {
        let run = match (of, hook) {
            (ListsOf::Action(action), _) => {
                let list = match hook {
                    Hook::OnResolve => 0,
                    Hook::OnHit => 1,
                    Hook::OnEnd => 2,
                    _ => unreachable!("only a resolve and a delivery's hit and end have lists"),
                };
                self.runs.get(action.index()).map(|runs| &runs[list])
            }
            (ListsOf::Launch(launch), Hook::OnHit | Hook::OnEnd) => {
                let list = usize::from(hook == Hook::OnEnd);
                Some(&self.launches[launch.0 as usize][list])
            }
            (ListsOf::Launch(_), _) => unreachable!("a launch's area has a hit and an end alone"),
        };
        run.map_or(&[], |run| {
            &self.effects[run.start as usize..run.end as usize]
        })
    }

    /// Whether the match loaded `launch`.
    pub(crate) fn has_launch(&self, launch: LaunchId) -> bool {
        (launch.0 as usize) < self.launches.len()
    }

    /// Queues the list of `of` that runs before its `hook` in `frame`, a call of its action at
    /// its rank, which `world` runs: each effect to the unit `reached`, what the list reached, or
    /// to the acting unit; a spawn where that unit stands, or at the point `reached` is. A
    /// modifier, a purge and a spawn queue here, as `stats` and the core are below the action
    /// pipeline; every other effect queues as its capability registered, which can fail the
    /// call, as experience to a unit without the track fails `ctx.add_xp`.
    pub(crate) fn queue(
        world: &World,
        of: ListsOf,
        hook: Hook,
        frame: &mut Frame,
        view: &View,
        reached: ActionTarget,
    ) -> Result<(), CallError> {
        let list = world.resource::<EffectLists>().of(of, hook);
        if list.is_empty() {
            return Ok(());
        }
        let rate = *world.resource::<TickRate>();
        let queues = world.resource::<EffectQueues>();
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

impl Amount {
    /// `number`, its param resolved by `names`, which the load checked a sim number.
    fn of(number: &Number, names: &impl EffectNames) -> Amount {
        match number {
            Number::Value(value) => Amount::Value(
                value
                    .to_num()
                    .expect("the load checked each number of an effect list"),
            ),
            Number::Param(reference) => Amount::Param(names.param(&reference.param)),
        }
    }

    /// Its value in `frame`, a call of its action: 0 for a scaling param below zero, which its
    /// source's stats can make it, as the load checks every other number not negative.
    pub(crate) fn number(self, frame: &Frame) -> Num {
        match self {
            Amount::Value(value) => value,
            Amount::Param(at) => StatsCall::ability_value(frame, at)
                .to_num()
                .expect("the load checked that an effect's param is a number")
                .max(Num::ZERO),
        }
    }
}

/// A position in the buffer, which a match's data keeps within `u32`.
fn position(len: usize) -> u32 {
    u32::try_from(len).expect("a match's effects fit u32")
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::actions::action_data::ActionData;
    use crate::actions::effect_lists::{EffectLists, LaunchId};
    use crate::actions::effect_names::EffectNames;
    use crate::progression::progression_column::ProgressionColumn;
    use crate::stats::Stats;
    use crate::stats::param_book::ParamBook;
    use crate::stats::pool_id::PoolId;
    use crate::stats::stats_column::StatsColumn;
    use crate::units::action_id::ActionId;
    use crate::units::modifier_id::ModifierId;
    use crate::units::script_view::View;
    use crate::units::tag::Tag;
    use crate::units::track_id::TrackId;
    use crate::units::type_scope::TypeScope;
    use crate::units::unit_type::UnitType;
    use crate::values::damage_kind::DamageKind;
    use crate::values::declared_name::DeclaredName;
    use bevy_ecs::world::World;

    impl LaunchId {
        /// The launch at `at` among the match's launches.
        pub(crate) const fn nth(at: u32) -> LaunchId {
            LaunchId(at)
        }
    }

    impl EffectLists {
        /// Loads the effect lists of `action` of `package`, which loaded last from `data`, which
        /// the package load checked: each name resolved to its id, each param to its place among
        /// the action's params.
        pub(crate) fn load(world: &mut World, action: ActionId, package: u16, data: &ActionData) {
            let view = world.non_send::<View>().clone();
            let mut lists = world.remove_resource::<EffectLists>().unwrap();
            let names = MatchEffectNames {
                world,
                view: &view,
                action,
                package,
            };
            lists.push(action, data, &names);
            world.insert_resource(lists);
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

        fn delivery_type(&self, name: &DeclaredName) -> UnitType {
            let scope = TypeScope::of_package(self.package);
            self.view
                .types_mut()
                .named(scope, name.as_str())
                .expect("the load checked a launch's area type")
        }

        fn standing_type(&self, name: &DeclaredName) -> UnitType {
            let scope = TypeScope::of_package(self.package);
            self.view
                .types_mut()
                .named(scope, name.as_str())
                .expect("the load checked a spawn's unit type")
        }
    }
}
