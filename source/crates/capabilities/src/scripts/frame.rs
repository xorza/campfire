use bevy_ecs::world::World;
use campfire_sim::{Capability, IdAllocator, StableId, Tick};

use crate::actions::action_book::ActionId;
use crate::mode::choices::Choices;
use crate::mode::match_end::MatchEnd;
use crate::mode::mode_state::ModeState;
use crate::mode::player_resources::PlayerResources;
use crate::scripts::call_start::CallStart;
use crate::scripts::effects::{ApplyEffect, Effects};
use crate::scripts::error::CallError;
use crate::scripts::hook::ScriptRole;
use crate::scripts::state_value::StateValue;
use crate::stats::Stats;
use crate::stats::modifier_book::ModifierId;
use crate::stats::modifier_handle::ModifierHandle;
use crate::stats::param_book::ParamBook;
use crate::stats::param_source::ParamSource;
use crate::stats::param_table::ParamTable;

use crate::values::hit::Hit;

use crate::values::scalar::Scalar;

/// What the running call reads and queues, beside the units the view holds: its role, its
/// acting unit, its params, the mode's state as a mode call sees it, and the effects it
/// queued. One frame serves the whole match, its buffers cleared and filled again, so a call
/// allocates none of them.
#[derive(Debug, Default)]
pub(crate) struct Frame {
    /// Every loaded ability's and modifier's params, which the stat engine reads too.
    params: ParamBook,
    /// The running call's role; none between calls.
    role: Option<ScriptRole>,
    /// Its acting unit: a cast's caster, a modifier's source, the unit that thinks; none for the
    /// mode.
    acting: Option<StableId>,
    /// The ability whose params it reads, at `rank`, and its modifier's.
    ability: Option<ActionId>,
    rank: u8,
    modifier: Option<ModifierId>,
    /// The depth of the chain of combat events it runs in: 0 for a cast.
    depth: u8,
    /// The package whose names it means: 0 the mode's.
    package: u16,
    /// The hit a delivery's hook runs for.
    hit: Option<Hit>,
    /// Its params at its rank, in the order of their names: its ability's, and its modifier's.
    values: Vec<Scalar>,
    modifier_values: Vec<Scalar>,
    /// Whether it is a pure hook's, whose `ctx` only reads.
    pub(crate) pure: bool,
    /// The mode's state and players' choices as a mode call sees them, which it writes and reads
    /// back; the stable ids as a mode call takes them for the units it spawns; whether the match
    /// ended, before the call or in it.
    pub(crate) state: Vec<StateValue>,
    pub(crate) choices: Choices,
    pub(crate) ids: IdAllocator,
    pub(crate) ended: bool,
    /// The players' resources as the call sees them, when the match has them, and whether the
    /// call changed them.
    resources: Option<PlayerResources>,
    resources_written: bool,
    pub(crate) effects: Effects,
    /// How each capability applies its effects, by capability index; the capability table
    /// gives it at install.
    dispatch: [Option<ApplyEffect>; Capability::ALL.len()],
    /// The modifier handles the call took, which write back when it applies.
    pub(crate) handles: Vec<ModifierHandle>,
}

impl Frame {
    /// Sets the params of every ability and every modifier, as the load built them.
    pub(crate) fn set_params(&mut self, params: ParamBook) {
        self.params = params;
    }

    pub(crate) const fn role(&self) -> Option<ScriptRole> {
        self.role
    }

    pub(crate) const fn acting(&self) -> Option<StableId> {
        self.acting
    }

    /// The action whose params it reads, and at which rank.
    pub(crate) const fn action(&self) -> Option<ActionId> {
        self.ability
    }

    pub(crate) const fn rank(&self) -> u8 {
        self.rank
    }

    pub(crate) const fn depth(&self) -> u8 {
        self.depth
    }

    pub(crate) const fn package(&self) -> u16 {
        self.package
    }

    pub(crate) const fn hit(&self) -> Option<Hit> {
        self.hit
    }

    /// Takes how each capability applies its effects, by capability index.
    pub(crate) const fn set_dispatch(
        &mut self,
        dispatch: [Option<ApplyEffect>; Capability::ALL.len()],
    ) {
        self.dispatch = dispatch;
    }

    /// Starts a cast of `ability` of `package` at `rank` by `caster` in `world`, or a hook of its
    /// delivery for `hit`, with its params at that rank; a param that overflows there fails the
    /// cast.
    pub(crate) fn begin_cast(
        &mut self,
        world: &World,
        ability: ActionId,
        rank: u8,
        caster: StableId,
        package: u16,
        hit: Option<Hit>,
    ) -> Result<(), CallError> {
        self.read_resources(world);
        let source = ParamSource::of(world, caster);
        let start = CallStart {
            role: ScriptRole::Action,
            acting: Some(caster),
            action: Some(ability),
            rank,
            modifier: None,
            package,
            depth: 0,
            hit,
        };
        self.begin(start, source.as_ref())
    }

    /// Starts a hook of `modifier` of `package` at chain depth `depth`, whose instance came from
    /// `source` by `ability` at `rank`: the modifier's params, then the ability's, at that rank;
    /// a param that overflows there fails the call.
    #[expect(
        clippy::too_many_arguments,
        reason = "a hook's parts, each from the instance or the event that runs it"
    )]
    pub(crate) fn begin_hook(
        &mut self,
        world: &World,
        modifier: ModifierId,
        ability: Option<ActionId>,
        rank: u8,
        source: Option<StableId>,
        package: u16,
        depth: u8,
    ) -> Result<(), CallError> {
        self.read_resources(world);
        let from = source.and_then(|source| ParamSource::of(world, source));
        let start = CallStart {
            role: ScriptRole::Modifier,
            acting: source,
            action: ability,
            rank,
            modifier: Some(modifier),
            package,
            depth,
            hit: None,
        };
        self.begin(start, from.as_ref())
    }

    /// Starts `on_think` for `unit` in `world`.
    pub(crate) fn begin_think(&mut self, world: &World, unit: StableId) {
        self.read_resources(world);
        let start = CallStart {
            acting: Some(unit),
            ..CallStart::mode(ScriptRole::Ai)
        };
        self.begin(start, None)
            .expect("a call with no params overflows none");
    }

    /// Starts a mode call, with the mode's state, choices, resources and stable ids as `world`
    /// holds them, for the call to read and write; `pure` for a hook whose `ctx` only reads.
    pub(crate) fn begin_mode(&mut self, world: &World, pure: bool) {
        self.read_resources(world);
        self.begin(CallStart::mode(ScriptRole::Mode), None)
            .expect("a call with no params overflows none");
        self.state.clear();
        self.state
            .extend_from_slice(&world.resource::<ModeState>().0);
        self.choices.clone_from(world.resource::<Choices>());
        self.ids.clone_from(world.resource::<IdAllocator>());
        self.ended = world.contains_resource::<MatchEnd>();
        self.pure = pure;
    }

    /// Reads the players' resources as `world` holds them, for the call to add to; none in a
    /// match with no mode.
    fn read_resources(&mut self, world: &World) {
        self.resources_written = false;
        match (world.get_resource::<PlayerResources>(), &mut self.resources) {
            (Some(held), Some(resources)) => resources.clone_from(held),
            (held, resources) => *resources = held.cloned(),
        }
    }

    /// The players' resources, for the call to change; `None` in a match with no mode.
    pub(crate) fn resources_mut(&mut self) -> Option<&mut PlayerResources> {
        self.resources_written = true;
        self.resources.as_mut()
    }

    /// Starts the call `start`, its params at its rank read from `source`.
    fn begin(
        &mut self,
        start: CallStart,
        source: Option<&ParamSource<'_>>,
    ) -> Result<(), CallError> {
        let CallStart {
            role,
            acting,
            action: ability,
            rank,
            modifier,
            package,
            depth,
            hit,
        } = start;
        self.role = Some(role);
        self.acting = acting;
        self.ability = ability;
        self.rank = rank;
        self.modifier = modifier;
        self.package = package;
        self.depth = depth;
        self.hit = hit;
        self.pure = false;
        self.effects.clear();
        self.handles.clear();
        let fill = |values: &mut Vec<Scalar>, table: &ParamTable, run: Option<usize>| {
            values.clear();
            let Some(run) = run else {
                return Ok(());
            };
            let count = table.len(run);
            values.reserve_exact(count);
            for at in 0..count {
                let value = table.value(run, at, rank, source);
                values.push(value.ok_or(CallError::ParamOverflow)?);
            }
            Ok(())
        };
        let run = ability.map(ActionId::index);
        fill(&mut self.values, self.params.actions(), run)?;
        let run = modifier.map(ModifierId::index);
        fill(&mut self.modifier_values, self.params.modifiers(), run)
    }

    /// Applies the effects the call that ran queued, in order, each by its capability, from
    /// the call's acting unit and its ability at its rank, in tick `now`. Then what the call
    /// wrote to modifier handles applies.
    pub(crate) fn apply(&mut self, world: &mut World, now: Tick) {
        for at in 0..self.effects.order().len() {
            let capability = self.effects.order()[at];
            let apply = self.dispatch[capability as usize]
                .expect("a capability that queues effects applies them");
            apply(world, self, now);
        }
        self.effects.clear();
        for handle in self.handles.drain(..) {
            Stats::write_handle(world, &handle);
        }
        if let (true, Some(resources)) = (self.resources_written, &self.resources) {
            world
                .resource_mut::<PlayerResources>()
                .clone_from(resources);
        }
    }

    /// The running call's ability's param at `at`, at its rank.
    pub(crate) fn ability_value(&self, at: usize) -> Scalar {
        self.values[at]
    }

    /// The running call's param `name`: its modifier's, then its ability's, if either declares
    /// one.
    pub(crate) fn param(&self, name: &str) -> Option<Scalar> {
        let own = self.modifier.and_then(|modifier| {
            let at = self.params.modifiers().find(modifier.index(), name)?;
            Some(self.modifier_values[at])
        });
        own.or_else(|| {
            let at = self.params.actions().find(self.ability?.index(), name)?;
            Some(self.values[at])
        })
    }
}
