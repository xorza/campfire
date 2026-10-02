use std::collections::BTreeMap;

use bevy_ecs::world::World;
use campfire_math::Num;
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
use crate::stats::live_param::{LiveParam, ParamOwner};
use crate::stats::modifier_book::ModifierId;
use crate::stats::modifier_handle::ModifierHandle;
use crate::stats::param_read::ParamRead;
use crate::stats::param_source::ParamSource;
use crate::stats::param_table::ParamTable;
use crate::stats::stat::Stat;
use crate::values::param::Param;
use crate::values::scalar::Scalar;

/// What the running call reads and queues, beside the units the view holds: its role, its
/// acting unit, its params, the mode's state as a mode call sees it, and the effects it
/// queued. One frame serves the whole match, its buffers cleared and filled again, so a call
/// allocates none of them.
#[derive(Debug, Default)]
pub(crate) struct Frame {
    /// Every loaded ability's params, one run per ability, by ability id; and every loaded
    /// modifier's, by modifier id.
    params: ParamTable,
    modifier_params: ParamTable,
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
    /// Adds the params of `ability`, the one the book loads next, each stat at its place `stat`
    /// gives.
    pub(crate) fn add_params(
        &mut self,
        ability: ActionId,
        params: &BTreeMap<String, Param>,
        stat: impl Fn(&Stat) -> u16,
    ) {
        let run = self.params.push(params, stat);
        debug_assert_eq!(run, ability.index(), "one run of params per ability");
    }

    /// Adds the params of `modifier`, the one the book loaded last, each stat at its place
    /// `stat` gives.
    pub(crate) fn add_modifier_params(
        &mut self,
        modifier: ModifierId,
        params: &BTreeMap<String, Param>,
        stat: impl Fn(&Stat) -> u16,
    ) {
        let run = self.modifier_params.push(params, stat);
        debug_assert_eq!(run, modifier.index(), "one run of params per modifier");
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

    /// Takes how each capability applies its effects, by capability index.
    pub(crate) const fn set_dispatch(
        &mut self,
        dispatch: [Option<ApplyEffect>; Capability::ALL.len()],
    ) {
        self.dispatch = dispatch;
    }

    /// Starts a cast of `ability` of `package` at `rank` by `caster` in `world`, with its params
    /// at that rank; a param that overflows there fails the cast.
    pub(crate) fn begin_cast(
        &mut self,
        world: &World,
        ability: ActionId,
        rank: u8,
        caster: StableId,
        package: u16,
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
        } = start;
        self.role = Some(role);
        self.acting = acting;
        self.ability = ability;
        self.rank = rank;
        self.modifier = modifier;
        self.package = package;
        self.depth = depth;
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
        fill(&mut self.values, &self.params, run)?;
        let run = modifier.map(ModifierId::index);
        fill(&mut self.modifier_values, &self.modifier_params, run)
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

    /// Param `name` a modifier's number reads: `modifier`'s own, then that of `ability`, which
    /// applied it, at `rank`, of `source`; `None` when neither declares it or it does not resolve.
    /// A scaling table's is live, read again as its source changes.
    pub(crate) fn modifier_param(
        &self,
        modifier: ModifierId,
        ability: Option<ActionId>,
        rank: u8,
        name: &str,
        source: Option<&ParamSource<'_>>,
    ) -> Option<ParamRead> {
        let own = self
            .modifier_params
            .find(modifier.index(), name)
            .map(|at| (ParamOwner::Modifier(modifier), at));
        let (owner, at) = own.or_else(|| {
            let ability = ability?;
            let at = self.params.find(ability.index(), name)?;
            Some((ParamOwner::Action(ability), at))
        })?;
        let (table, run) = self.table(owner);
        let value = table.value(run, at, rank, source)?.to_num()?;
        let live = table.scales(run, at).then(|| LiveParam {
            owner,
            at: u16::try_from(at).expect("params fit u16"),
        });
        Some(ParamRead { value, live })
    }

    /// The value of live param `live` at `rank` of `source`.
    pub(crate) fn live_value(
        &self,
        live: LiveParam,
        rank: u8,
        source: Option<&ParamSource<'_>>,
    ) -> Option<Num> {
        let (table, run) = self.table(live.owner);
        table
            .value(run, usize::from(live.at), rank, source)?
            .to_num()
    }

    /// The table of `owner`'s params, and its run there.
    fn table(&self, owner: ParamOwner) -> (&ParamTable, usize) {
        match owner {
            ParamOwner::Modifier(modifier) => (&self.modifier_params, modifier.index()),
            ParamOwner::Action(ability) => (&self.params, ability.index()),
        }
    }

    /// The running call's param `name`: its modifier's, then its ability's, if either declares
    /// one.
    pub(crate) fn param(&self, name: &str) -> Option<Scalar> {
        let own = self.modifier.and_then(|modifier| {
            let at = self.modifier_params.find(modifier.index(), name)?;
            Some(self.modifier_values[at])
        });
        own.or_else(|| {
            let at = self.params.find(self.ability?.index(), name)?;
            Some(self.values[at])
        })
    }
}
