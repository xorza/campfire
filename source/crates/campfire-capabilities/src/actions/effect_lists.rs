use std::ops::Range;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::{Capability, StableId, TickRate};

use crate::actions::action_data::ActionData;
use crate::actions::effect_data::{EffectData, EffectTo, Effecting};
use crate::actions::effect_names::EffectNames;
use crate::actions::effect_queues::EffectQueues;
use crate::scripts::error::CallError;
use crate::scripts::frame::Frame;
use crate::scripts::hook::Hook;
use crate::stats::modifier_effect::ModifierEffect;
use crate::stats::pool_id::PoolId;
use crate::stats::stats_call::StatsCall;
use crate::units::action_id::ActionId;
use crate::units::modifier_id::ModifierId;
use crate::units::script_view::View;
use crate::units::tag::Tag;
use crate::units::track_id::TrackId;
use crate::values::damage_kind::DamageKind;
use crate::values::number::Number;

/// The effect lists of each action, their names resolved as the action loaded: one buffer, and
/// by action id the run of each of its lists, `on_resolve`, `on_hit` and `on_end`. An action
/// past the end has none. Package data, not state.
#[derive(Resource, Debug, Default)]
pub(crate) struct EffectLists {
    effects: Vec<Listed>,
    runs: Vec<[Range<u32>; 3]>,
}

/// An effect of a list: what it does, and to whom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Listed {
    pub(crate) does: Does,
    pub(crate) to: EffectTo,
}

/// What a listed effect does, its names resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Does {
    Damage {
        amount: Amount,
        kind: DamageKind,
    },
    Heal {
        amount: Amount,
    },
    Restore {
        pool: PoolId,
        amount: Amount,
    },
    Modifier {
        id: ModifierId,
        duration_ms: Option<Amount>,
    },
    Xp {
        track: TrackId,
        amount: Amount,
    },
    Purge {
        tag: Tag,
    },
}

/// A number of a listed effect: a value, or the param at its place among its action's, which the
/// frame holds at the call's rank.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Amount {
    Value(Num),
    Param(usize),
}

impl EffectLists {
    /// Adds the lists of `action`, which loaded last: `on_resolve`, `on_hit` and `on_end`.
    pub(crate) fn push(&mut self, action: ActionId, lists: [Vec<Listed>; 3]) {
        if self.runs.len() <= action.index() {
            self.runs.resize(action.index() + 1, [0..0, 0..0, 0..0]);
        }
        self.runs[action.index()] = lists.map(|list| {
            let start = position(self.effects.len());
            self.effects.extend(list);
            start..position(self.effects.len())
        });
    }

    /// The list of `action` that runs before its `hook`: `on_resolve`, `on_hit` or `on_end`.
    pub(crate) fn of(&self, action: ActionId, hook: Hook) -> &[Listed] {
        let list = match hook {
            Hook::OnResolve => 0,
            Hook::OnHit => 1,
            Hook::OnEnd => 2,
            _ => unreachable!("only a resolve and a delivery's hit and end have lists"),
        };
        self.runs.get(action.index()).map_or(&[], |runs| {
            let run = &runs[list];
            &self.effects[run.start as usize..run.end as usize]
        })
    }

    /// Queues the list of `action` that runs before its `hook` in `frame`, a call of the action
    /// at its rank, which `world` runs: each effect to `reached`, the unit the list reached, or to
    /// the acting unit. A modifier and a purge queue here, as `stats` is below the action pipeline; every
    /// other effect queues as its capability registered, which can fail the call, as experience
    /// to a unit without the track fails `ctx.add_xp`.
    pub(crate) fn queue(
        world: &World,
        action: ActionId,
        hook: Hook,
        frame: &mut Frame,
        view: &View,
        reached: Option<StableId>,
    ) -> Result<(), CallError> {
        let list = world.resource::<EffectLists>().of(action, hook);
        if list.is_empty() {
            return Ok(());
        }
        let rate = *world.resource::<TickRate>();
        let queues = world.resource::<EffectQueues>();
        let acting = frame.acting();
        for &listed in list {
            let unit = match listed.to {
                EffectTo::Reached => {
                    reached.expect("the load lets only an effect to the source reach no unit")
                }
                EffectTo::Source => acting.expect("an action's list runs for its acting unit"),
            };
            match listed.does {
                Does::Modifier { id, duration_ms } => {
                    let duration = duration_ms.map(|ms| {
                        // Whole, as the load checked, so the floor is exact.
                        let ms = ms.number(frame).floor();
                        let ms =
                            u64::try_from(ms).expect("the load checked a duration not negative");
                        rate.duration(ms)
                            .expect("the load checked a duration within reach")
                    });
                    frame.effects.push(ModifierEffect::Add {
                        target: unit,
                        id,
                        duration,
                    });
                }
                Does::Purge { tag } => frame
                    .effects
                    .push(ModifierEffect::Purge { carrier: unit, tag }),
                does => queues.of(does.capability())(does, unit, frame, view)?,
            }
        }
        Ok(())
    }
}

impl Does {
    /// The capability whose effect it is.
    pub(crate) const fn capability(self) -> Capability {
        match self {
            Does::Damage { .. } | Does::Heal { .. } | Does::Restore { .. } => Capability::Combat,
            Does::Modifier { .. } | Does::Purge { .. } => Capability::Stats,
            Does::Xp { .. } => Capability::Progression,
        }
    }
}

impl Listed {
    /// The lists of `data`, `on_resolve`, `on_hit` and `on_end`, which the package load checked,
    /// their names resolved by `names`.
    pub(crate) fn lists_of(data: &ActionData, names: &impl EffectNames) -> [Vec<Listed>; 3] {
        let amount = |number: &Number| match number {
            Number::Value(value) => Amount::Value(
                value
                    .to_num()
                    .expect("the load checked each number of an effect list"),
            ),
            Number::Param(reference) => Amount::Param(names.param(&reference.param)),
        };
        let resolve = |effect: &EffectData| {
            let does = match &effect.does {
                Effecting::Damage {
                    amount: number,
                    kind,
                } => Does::Damage {
                    amount: amount(number),
                    kind: names.damage_kind(kind),
                },
                Effecting::Heal { amount: number } => Does::Heal {
                    amount: amount(number),
                },
                Effecting::Restore {
                    pool,
                    amount: number,
                } => Does::Restore {
                    pool: names.pool(pool),
                    amount: amount(number),
                },
                Effecting::Modifier { id, duration_ms } => Does::Modifier {
                    id: names.modifier(id),
                    duration_ms: duration_ms.as_ref().map(amount),
                },
                Effecting::Xp {
                    track,
                    amount: number,
                } => Does::Xp {
                    track: names.track(track),
                    amount: amount(number),
                },
                Effecting::Purge { tag } => Does::Purge {
                    tag: names.tag(tag),
                },
                Effecting::Planned(_) => unreachable!("the load refuses a planned effect"),
            };
            Listed {
                does,
                to: effect.to,
            }
        };
        [&data.on_resolve, &data.on_hit, &data.on_end]
            .map(|list| list.iter().map(resolve).collect::<Vec<_>>())
    }
}

impl Amount {
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
    use crate::actions::effect_lists::{EffectLists, Listed};
    use crate::actions::effect_names::EffectNames;
    use crate::progression::tracks_column::TracksColumn;
    use crate::stats::Stats;
    use crate::stats::param_book::ParamBook;
    use crate::stats::pool_id::PoolId;
    use crate::stats::stats_column::StatsColumn;
    use crate::units::action_id::ActionId;
    use crate::units::modifier_id::ModifierId;
    use crate::units::script_view::View;
    use crate::units::tag::Tag;
    use crate::units::track_id::TrackId;
    use crate::values::damage_kind::DamageKind;
    use crate::values::declared_name::DeclaredName;
    use bevy_ecs::world::World;

    impl EffectLists {
        /// Loads the effect lists of `action` of `package`, which loaded last from `data`, which
        /// the package load checked: each name resolved to its id, each param to its place among
        /// the action's params.
        pub(crate) fn load(world: &mut World, action: ActionId, package: u16, data: &ActionData) {
            let view = world.non_send::<View>().clone();
            let names = MatchEffectNames {
                world,
                view: &view,
                action,
                package,
            };
            let lists = Listed::lists_of(data, &names);
            world.resource_mut::<EffectLists>().push(action, lists);
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
            TracksColumn::track_named(self.view, name.as_str())
                .expect("the load checked an effect's track")
        }

        fn tag(&self, name: &DeclaredName) -> Tag {
            self.view
                .tag_named(name.as_str())
                .expect("the load checked an effect's tag")
        }
    }
}
