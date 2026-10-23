use std::ops::Range;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_declarations::Book;
use campfire_script::{CallError, Ctx, MatchNames};
use campfire_values::{ActionId, NameId};

use crate::action_data::ActionData;
use crate::action_target::ActionTarget;
use crate::effect_names::EffectNames;
use crate::error::ActionError;
use crate::launch_id::LaunchId;
use crate::list_hook::ListHook;
use crate::listed_effect::EffectResolveCx;
use crate::listed_effect::ListedEffect;
use crate::listed_effect::effect_data::EffectData;
use crate::listed_effect::effect_data::LaunchLists;
use crate::listed_effect::error::{EffectError, EffectRule};
use crate::listed_effect::listed_spec::EffectTo;
use crate::listed_effect::listed_spec::{ListedAt, ListedSpec};
use crate::lists_of::ListsOf;

/// The effect lists of each action, their names resolved as the action loaded: one buffer, by
/// action id the run of each of its lists, `on_resolve`, `on_hit` and `on_end`, and by launch id
/// the run of each list a launch holds, `on_hit` and `on_end`. An action past the end has none.
/// Package data, not state, which a clone shares.
#[derive(Resource, Debug, Clone, Default)]
#[component(immutable)]
pub struct EffectListBook {
    effects: Arc<[EffectSpec]>,
    runs: Arc<[[Range<u32>; 3]]>,
    launches: Arc<[[Range<u32>; 2]]>,
}

/// An `EffectListBook` as the load fills it, action by action.
#[derive(Debug, Default)]
pub(crate) struct EffectListBuilder {
    effects: Vec<EffectSpec>,
    runs: Vec<[Range<u32>; 3]>,
    launches: Vec<[Range<u32>; 2]>,
}

/// An effect of a list: what it does, its module's own, and to whom.
#[derive(Debug, Clone)]
pub struct EffectSpec {
    pub(crate) does: Arc<dyn ListedSpec>,
    pub(crate) to: EffectTo,
}

/// The list a call runs: the list of `of` that runs before its `hook`, and what it reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListCall {
    pub of: ListsOf,
    pub reached: ActionTarget,
}

impl EffectListBuilder {
    pub(crate) fn build(self) -> EffectListBook {
        EffectListBook {
            effects: self.effects.into(),
            runs: self.runs.into(),
            launches: self.launches.into(),
        }
    }

    /// Adds the lists of `action`, which loaded last, from `data`, their names resolved by
    /// `names`: `on_resolve`, `on_hit` and `on_end`, and the lists each launch of them holds,
    /// however deep; an error for a name the match does not have, or a number or a duration that
    /// breaks its rule, which fails the whole load.
    pub(crate) fn push(
        &mut self,
        action: ActionId,
        data: &ActionData,
        names: EffectNames<'_>,
    ) -> Result<(), ActionError> {
        if self.runs.len() <= action.index() {
            self.runs.resize(action.index() + 1, [0..0, 0..0, 0..0]);
        }
        let mut runs = [0..0, 0..0, 0..0];
        let lists = data.effect_lists();
        for (run, (hook, list)) in runs.iter_mut().zip(lists) {
            *run = self.append(data, hook, list, names)?;
        }
        self.runs[action.index()] = runs;
        Ok(())
    }

    /// The launch of `lists`, of the action `data`, their names resolved by `names`.
    fn launch(
        &mut self,
        data: &ActionData,
        lists: &LaunchLists,
        names: EffectNames<'_>,
    ) -> Result<LaunchId, EffectError> {
        let lists = [
            self.append(data, ListHook::OnHit, &lists.on_hit, names)?,
            self.append(data, ListHook::OnEnd, &lists.on_end, names)?,
        ];
        let launch = LaunchId(position(self.launches.len()));
        self.launches.push(lists);
        Ok(launch)
    }

    /// Appends `list`, the action `data`'s list `hook`, its names resolved by `names`, after the
    /// lists its launches hold, so its own effects lie together; its run.
    fn append(
        &mut self,
        data: &ActionData,
        hook: ListHook,
        list: &[EffectData],
        names: EffectNames<'_>,
    ) -> Result<Range<u32>, EffectError> {
        let resolved: Vec<EffectSpec> = list
            .iter()
            .map(|effect| {
                Self::holds(data, hook, &*effect.does)?;
                let mut cx = ListResolve {
                    book: self,
                    data,
                    hook,
                    names,
                };
                Ok(EffectSpec {
                    does: effect.does.resolve(&mut cx)?,
                    to: effect.to,
                })
            })
            .collect::<Result<_, EffectError>>()?;
        let start = position(self.effects.len());
        self.effects.extend(resolved);
        Ok(start..position(self.effects.len()))
    }

    /// Whether the numbers of `does`, an effect of the action `data`'s list `hook`, hold their
    /// rules: its duration whole milliseconds, then each number a sim number at least 0.
    fn holds(
        data: &ActionData,
        hook: ListHook,
        does: &dyn ListedEffect,
    ) -> Result<(), EffectError> {
        let broken = |rule| EffectError::Rule { list: hook, rule };
        if does
            .duration()
            .is_some_and(|duration| !data.whole_ms(duration))
        {
            return Err(broken(EffectRule::Duration));
        }
        for number in does.numbers() {
            data.number_rule(number).map_err(broken)?;
        }
        Ok(())
    }
}

impl EffectListBook {
    /// The list `of` names: an action's `on_resolve`, `on_hit` or `on_end`, or a launch's
    /// `on_hit` or `on_end`.
    pub fn of(&self, of: ListsOf) -> &[EffectSpec] {
        let run = match of {
            ListsOf::Action(action, hook) => self
                .runs
                .get(action.index())
                .map(|runs| &runs[hook.index()]),
            ListsOf::Launch(launch, hook) => Some(&self.launches[launch.0 as usize][hook.index()]),
        };
        run.map_or(&[], |run| {
            &self.effects[run.start as usize..run.end as usize]
        })
    }

    /// Whether the match loaded `launch`.
    pub fn has_launch(&self, launch: LaunchId) -> bool {
        (launch.0 as usize) < self.launches.len()
    }

    /// Queues the list of `call` in the call `ctx` runs, of the list's action at its rank, which
    /// the match runs: each effect to the unit the list reached, or to the acting unit, as its
    /// module queues it, which can fail the call, as experience to a unit without the track fails
    /// `ctx.add_xp`.
    pub fn queue(world: &World, call: ListCall, ctx: &Ctx) -> Result<(), CallError> {
        let ListCall { of, reached } = call;
        let mut held = ctx.frame();
        let (frame, view) = (&mut *held, ctx.view());
        for listed in world.resource::<Self>().of(of) {
            let at = ListedAt {
                to: listed.to,
                reached,
            };
            listed.does.queue(at, frame, view)?;
        }
        Ok(())
    }
}

/// The resolve of an effect of the action `data`'s list `hook`, its names resolved by `names`,
/// into `book`.
#[derive(Debug)]
struct ListResolve<'r> {
    book: &'r mut EffectListBuilder,
    data: &'r ActionData,
    hook: ListHook,
    names: EffectNames<'r>,
}

impl EffectResolveCx for ListResolve<'_> {
    fn names(&self) -> EffectNames<'_> {
        self.names
    }

    fn list(&self) -> ListHook {
        self.hook
    }

    fn launch(&mut self, lists: &LaunchLists) -> Result<LaunchId, EffectError> {
        self.book.launch(self.data, lists, self.names)
    }
}

/// A position in the buffer, which a match's data keeps within `u32`.
fn position(len: usize) -> u32 {
    u32::try_from(len).expect("a match's effects fit u32")
}

impl Book for EffectListBook {
    const SCRIPTS_ONLY: bool = true;

    fn share(&self, _: &mut MatchNames) {}
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_values::ActionId;

    use crate::action_data::ActionData;
    use crate::effect_list_book::{EffectListBook, EffectListBuilder};
    use crate::effect_names::EffectNames;
    use crate::error::ActionError;
    use crate::launch_id::LaunchId;

    impl EffectListBook {
        /// The book with the lists of `action` added, as a load adds them, for a test that
        /// loads an action into a match built before it.
        pub(crate) fn pushed(
            &self,
            action: ActionId,
            data: &ActionData,
            names: EffectNames<'_>,
        ) -> Result<Self, ActionError> {
            let mut builder = EffectListBuilder {
                effects: self.effects.to_vec(),
                runs: self.runs.to_vec(),
                launches: self.launches.to_vec(),
            };
            builder.push(action, data, names)?;
            Ok(builder.build())
        }
    }

    impl LaunchId {
        /// The launch at `at` among the match's launches.
        pub const fn nth(at: u32) -> Self {
            Self(at)
        }
    }
}
