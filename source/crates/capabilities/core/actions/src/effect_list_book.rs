use std::ops::Range;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_declare::Book;
use campfire_math::Num;
use campfire_script::{CallError, Ctx, Frame, Hook, View};
use campfire_sim::{Capability, TickRate};
use campfire_stats::StatsEffect;
use campfire_units::SpawnAt;
use campfire_values::{ActionId, NameId, Number};

use crate::action_data::ActionData;
use crate::action_data::error::EffectRule;
use crate::action_target::ActionTarget;
use crate::actions_effect::ActionsEffect;
use crate::amount::Amount;
use crate::capability_does_spec::CapabilityDoesSpec;
use crate::does_spec::DoesSpec;
use crate::effect_data::{
    DamageFields, DoesData, EffectData, EffectTo, HealFields, LaunchFields, ModifierFields,
    MoveData, PurgeFields, RestoreFields, SpawnFields, XpFields,
};
use crate::effect_names::EffectNames;
use crate::effect_queues::EffectQueues;
use crate::error::{ActionError, ActionProblem};
use crate::launch_id::LaunchId;
use crate::lists_of::ListsOf;

/// The effect lists of each action, their names resolved as the action loaded: one buffer, by
/// action id the run of each of its lists, `on_resolve`, `on_hit` and `on_end`, and by launch id
/// the run of each list a launch holds, `on_hit` and `on_end`. An action past the end has none.
/// Package data, not state.
#[derive(Resource, Debug, Default)]
pub struct EffectListBook {
    effects: Vec<EffectSpec>,
    runs: Vec<[Range<u32>; 3]>,
    launches: Vec<[Range<u32>; 2]>,
}

/// An effect of a list: what it does, and to whom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectSpec {
    pub(crate) does: DoesSpec,
    pub(crate) to: EffectTo,
}

/// The list a call runs: the list of `of` that runs before its `hook`, and what it reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListCall {
    pub of: ListsOf,
    pub hook: Hook,
    pub reached: ActionTarget,
}

impl EffectListBook {
    /// Adds the lists of `action`, which loaded last, from `data`, their names resolved by
    /// `names`: `on_resolve`, `on_hit` and `on_end`, and the lists each launch of them holds,
    /// however deep; an error for a name the match does not have, or a number or a duration that
    /// breaks its rule, which fails the whole load.
    pub fn push(
        &mut self,
        action: ActionId,
        data: &ActionData,
        names: &impl EffectNames,
    ) -> Result<(), ActionError> {
        if self.runs.len() <= action.index() {
            self.runs.resize(action.index() + 1, [0..0, 0..0, 0..0]);
        }
        let mut runs = [0..0, 0..0, 0..0];
        let lists = [
            (Hook::OnResolve, &data.on_resolve),
            (Hook::OnHit, &data.on_hit),
            (Hook::OnEnd, &data.on_end),
        ];
        for (run, (hook, list)) in runs.iter_mut().zip(lists) {
            *run = self.append(data, hook, list, names)?;
        }
        self.runs[action.index()] = runs;
        Ok(())
    }

    /// Appends `list`, the action `data`'s list `hook`, its names resolved by `names`, after the
    /// lists its launches hold, so its own effects lie together; its run.
    fn append(
        &mut self,
        data: &ActionData,
        hook: Hook,
        list: &[EffectData],
        names: &impl EffectNames,
    ) -> Result<Range<u32>, ActionError> {
        let resolved: Vec<EffectSpec> = list
            .iter()
            .map(|effect| {
                EffectListBook::holds(data, hook, &effect.does)?;
                Ok(EffectSpec {
                    does: self.resolve(data, hook, &effect.does, names)?,
                    to: effect.to,
                })
            })
            .collect::<Result<_, ActionError>>()?;
        let start = position(self.effects.len());
        self.effects.extend(resolved);
        Ok(start..position(self.effects.len()))
    }

    /// Whether the numbers of `does`, an effect of the action `data`'s list `hook`, hold their
    /// rules: its duration whole milliseconds, then each number a sim number at least 0.
    fn holds(data: &ActionData, hook: Hook, does: &DoesData) -> Result<(), ActionProblem> {
        let broken = |rule| ActionProblem::Effect { list: hook, rule };
        let duration = match does {
            DoesData::Modifier(ModifierFields { duration_ms, .. })
            | DoesData::Spawn(SpawnFields { duration_ms, .. }) => duration_ms.as_ref(),
            DoesData::Move(MoveData::KnockBack { ms, .. }) => Some(ms),
            _ => None,
        };
        if duration.is_some_and(|duration| !data.whole_ms(duration)) {
            return Err(broken(EffectRule::Duration));
        }
        for number in does.numbers() {
            data.number_rule(number).map_err(broken)?;
        }
        Ok(())
    }

    /// What `does`, an effect of the action `data`'s list `hook`, does, its names resolved by
    /// `names`; a launch's lists appended first.
    fn resolve(
        &mut self,
        data: &ActionData,
        hook: Hook,
        does: &DoesData,
        names: &impl EffectNames,
    ) -> Result<DoesSpec, ActionError> {
        let amount = |number: &Number| Ok::<_, ActionError>(Amount::of(number, names)?);
        let maybe = |number: &Option<Number>| number.as_ref().map(amount).transpose();
        let of_capability = match does {
            DoesData::Modifier(ModifierFields { id, duration_ms }) => {
                return Ok(DoesSpec::Modifier {
                    id: names.modifier(id)?,
                    duration_ms: maybe(duration_ms)?,
                });
            }
            DoesData::Purge(PurgeFields { tag }) => {
                return Ok(DoesSpec::Purge {
                    tag: names.tag(tag)?,
                });
            }
            DoesData::Spawn(SpawnFields {
                unit_type,
                duration_ms,
            }) => {
                return Ok(DoesSpec::Spawn {
                    unit_type: names.package_type(unit_type)?,
                    duration_ms: maybe(duration_ms)?,
                });
            }
            DoesData::Damage(DamageFields {
                amount: dealt,
                kind,
            }) => CapabilityDoesSpec::Damage {
                amount: amount(dealt)?,
                kind: names.damage_kind(kind)?,
            },
            DoesData::Heal(HealFields { amount: healed }) => CapabilityDoesSpec::Heal {
                amount: amount(healed)?,
            },
            DoesData::Restore(RestoreFields {
                pool,
                amount: restored,
            }) => CapabilityDoesSpec::Restore {
                pool: names.pool(pool)?,
                amount: amount(restored)?,
            },
            DoesData::Xp(XpFields {
                track,
                amount: given,
            }) => CapabilityDoesSpec::Xp {
                track: names.track(track)?,
                amount: amount(given)?,
            },
            DoesData::Launch(LaunchFields {
                area,
                on_hit,
                on_end,
            }) => {
                let area = names.package_type(area)?;
                let lists = [
                    self.append(data, Hook::OnHit, on_hit, names)?,
                    self.append(data, Hook::OnEnd, on_end, names)?,
                ];
                let launch = LaunchId(position(self.launches.len()));
                self.launches.push(lists);
                CapabilityDoesSpec::Launch { area, launch }
            }
            DoesData::Move(MoveData::Dash { to, speed }) => CapabilityDoesSpec::Dash {
                to: *to,
                speed: amount(speed)?,
            },
            DoesData::Move(MoveData::KnockBack { from, distance, ms }) => {
                CapabilityDoesSpec::KnockBack {
                    from: *from,
                    distance: amount(distance)?,
                    ms: amount(ms)?,
                }
            }
            DoesData::Planned(planned) => {
                let rule = EffectRule::Planned(*planned);
                return Err(ActionProblem::Effect { list: hook, rule }.into());
            }
        };
        Ok(DoesSpec::Capability(of_capability))
    }

    /// The list of `of` that runs before its `hook`: an action's `on_resolve`, `on_hit` or
    /// `on_end`, or a launch's `on_hit` or `on_end`.
    pub fn of(&self, of: ListsOf, hook: Hook) -> &[EffectSpec] {
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
    pub const fn has_launch(&self, launch: LaunchId) -> bool {
        (launch.0 as usize) < self.launches.len()
    }

    /// Queues the list of `call` in the call `ctx` runs, of the list's action at its rank, which
    /// the match runs: each effect to the unit the list reached, or to the acting unit; a spawn
    /// where that unit stands, or at the point the list reached. A modifier, a purge and a spawn
    /// queue here, as `stats` and the core are below the action pipeline; every other effect
    /// queues as its capability declared, which can fail the call, as experience to a unit
    /// without the track fails `ctx.add_xp`.
    pub fn queue(world: &World, call: ListCall, ctx: &Ctx) -> Result<(), CallError> {
        let ListCall { of, hook, reached } = call;
        let mut held = ctx.frame();
        let (frame, view) = (&mut *held, ctx.view());
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
            let ms = u64::try_from(ms).expect("a duration's book holds it not negative");
            rate.duration(ms)
                .expect("a duration's book holds it within a u32 of milliseconds")
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
                        owner: source.owner,
                        pos,
                        angle: Num::ZERO,
                    };
                    let life = duration_ms.map(|ms| duration(ms, frame));
                    frame.queue(ActionsEffect { at, life });
                }
                DoesSpec::Modifier { id, duration_ms } => {
                    let duration = duration_ms.map(|ms| duration(ms, frame));
                    frame.queue(StatsEffect::Add {
                        target: unit(),
                        id,
                        duration,
                    });
                }
                DoesSpec::Purge { tag } => frame.queue(StatsEffect::Purge {
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
    const OWNER: Option<Capability> = None;
    const SCRIPTS_ONLY: bool = true;

    fn share(&self, _: &View) {}
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::launch_id::LaunchId;

    impl LaunchId {
        /// The launch at `at` among the match's launches.
        pub const fn nth(at: u32) -> LaunchId {
            LaunchId(at)
        }
    }
}
