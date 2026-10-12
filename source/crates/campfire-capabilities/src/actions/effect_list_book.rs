use std::ops::Range;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_sim::{Capability, TickRate};

use crate::actions::action_data::ActionData;
use crate::actions::action_target::ActionTarget;
use crate::actions::actions_effect::ActionsEffect;
use crate::actions::amount::Amount;
use crate::actions::capability_does_spec::CapabilityDoesSpec;
use crate::actions::does_spec::DoesSpec;
use crate::actions::effect_data::{
    DamageFields, DoesData, EffectData, EffectTo, HealFields, LaunchFields, ModifierFields,
    MoveData, PurgeFields, RestoreFields, SpawnFields, XpFields,
};
use crate::actions::effect_names::EffectNames;
use crate::actions::effect_queues::EffectQueues;
use crate::actions::launch_id::LaunchId;
use crate::actions::lists_of::ListsOf;
use crate::declare::book::Book;
use crate::scripts::error::CallError;
use crate::scripts::frame::Frame;
use crate::scripts::hook::Hook;
use crate::stats::stats_effect::StatsEffect;
use crate::units::action_id::ActionId;
use crate::units::spawn_at::SpawnAt;
use crate::units::view::View;

/// The effect lists of each action, their names resolved as the action loaded: one buffer, by
/// action id the run of each of its lists, `on_resolve`, `on_hit` and `on_end`, and by launch id
/// the run of each list a launch holds, `on_hit` and `on_end`. An action past the end has none.
/// Package data, not state.
#[derive(Resource, Debug, Default)]
pub(crate) struct EffectListBook {
    effects: Vec<EffectSpec>,
    runs: Vec<[Range<u32>; 3]>,
    launches: Vec<[Range<u32>; 2]>,
}

/// An effect of a list: what it does, and to whom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EffectSpec {
    pub(crate) does: DoesSpec,
    pub(crate) to: EffectTo,
}

impl EffectListBook {
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
        let resolved: Vec<EffectSpec> = list
            .iter()
            .map(|effect| EffectSpec {
                does: self.resolve(&effect.does, names),
                to: effect.to,
            })
            .collect();
        let start = position(self.effects.len());
        self.effects.extend(resolved);
        start..position(self.effects.len())
    }

    /// What `does` does, its names resolved by `names`; a launch's lists appended first.
    fn resolve(&mut self, does: &DoesData, names: &impl EffectNames) -> DoesSpec {
        let amount = |number| Amount::of(number, names);
        let of_capability = match does {
            DoesData::Modifier(ModifierFields { id, duration_ms }) => {
                return DoesSpec::Modifier {
                    id: names.modifier(id),
                    duration_ms: duration_ms.as_ref().map(amount),
                };
            }
            DoesData::Purge(PurgeFields { tag }) => {
                return DoesSpec::Purge {
                    tag: names.tag(tag),
                };
            }
            DoesData::Spawn(SpawnFields {
                unit_type,
                duration_ms,
            }) => {
                return DoesSpec::Spawn {
                    unit_type: names.package_type(unit_type),
                    duration_ms: duration_ms.as_ref().map(amount),
                };
            }
            DoesData::Damage(DamageFields {
                amount: dealt,
                kind,
            }) => CapabilityDoesSpec::Damage {
                amount: amount(dealt),
                kind: names.damage_kind(kind),
            },
            DoesData::Heal(HealFields { amount: healed }) => CapabilityDoesSpec::Heal {
                amount: amount(healed),
            },
            DoesData::Restore(RestoreFields {
                pool,
                amount: restored,
            }) => CapabilityDoesSpec::Restore {
                pool: names.pool(pool),
                amount: amount(restored),
            },
            DoesData::Xp(XpFields {
                track,
                amount: given,
            }) => CapabilityDoesSpec::Xp {
                track: names.track(track),
                amount: amount(given),
            },
            DoesData::Launch(LaunchFields {
                area,
                on_hit,
                on_end,
            }) => {
                let lists = [self.append(on_hit, names), self.append(on_end, names)];
                let launch = LaunchId(position(self.launches.len()));
                self.launches.push(lists);
                CapabilityDoesSpec::Launch {
                    area: names.package_type(area),
                    launch,
                }
            }
            DoesData::Move(MoveData::Dash { to, speed }) => CapabilityDoesSpec::Dash {
                to: *to,
                speed: amount(speed),
            },
            DoesData::Move(MoveData::KnockBack { from, distance, ms }) => {
                CapabilityDoesSpec::KnockBack {
                    from: *from,
                    distance: amount(distance),
                    ms: amount(ms),
                }
            }
            DoesData::Planned(_) => unreachable!("the load refuses a planned effect"),
        };
        DoesSpec::Capability(of_capability)
    }

    /// The list of `of` that runs before its `hook`: an action's `on_resolve`, `on_hit` or
    /// `on_end`, or a launch's `on_hit` or `on_end`.
    pub(crate) fn of(&self, of: ListsOf, hook: Hook) -> &[EffectSpec] {
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
    /// its rank, which the match runs: each effect to the unit `reached`, what the list reached, or
    /// to the acting unit; a spawn where that unit stands, or at the point `reached` is. A
    /// modifier, a purge and a spawn queue here, as `stats` and the core are below the action
    /// pipeline; every other effect queues as its capability declared, which can fail the
    /// call, as experience to a unit without the track fails `ctx.add_xp`.
    pub(crate) fn queue(
        world: &World,
        of: ListsOf,
        hook: Hook,
        frame: &mut Frame,
        view: &View,
        reached: ActionTarget,
    ) -> Result<(), CallError> {
        let list = world.resource::<EffectListBook>().of(of, hook);
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
                DoesSpec::Spawn {
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
                DoesSpec::Modifier { id, duration_ms } => {
                    let duration = duration_ms.map(|ms| duration(ms, frame));
                    frame.effects.push(StatsEffect::Add {
                        target: unit(),
                        id,
                        duration,
                    });
                }
                DoesSpec::Purge { tag } => frame.effects.push(StatsEffect::Purge {
                    carrier: unit(),
                    tag,
                }),
                DoesSpec::Capability(does) => {
                    queues.of(does.capability())(does, unit(), reached_unit, frame, view)?;
                }
            }
        }
        Ok(())
    }
}

/// A position in the buffer, which a match's data keeps within `u32`.
fn position(len: usize) -> u32 {
    u32::try_from(len).expect("a match's effects fit u32")
}

impl Book for EffectListBook {
    const CAPABILITY: Option<Capability> = None;
    const SCRIPTS_ONLY: bool = true;
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::actions::launch_id::LaunchId;

    impl LaunchId {
        /// The launch at `at` among the match's launches.
        pub(crate) const fn nth(at: u32) -> LaunchId {
            LaunchId(at)
        }
    }
}
