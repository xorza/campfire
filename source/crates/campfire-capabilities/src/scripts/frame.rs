use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_script::rhai::Dynamic;
use campfire_sim::{IdAllocator, StableId};

use crate::players::player_resources::PlayerResources;
use crate::scripts::call_part::{CallPart, CallParts};
use crate::scripts::call_start::CallStart;
use crate::scripts::effects::Effects;
use crate::scripts::error::{ApiError, CallError};
use crate::scripts::script_limits::ScriptLimits;
use crate::scripts::script_role::ScriptRole;
use crate::units::action_id::ActionId;
use crate::units::modifier_id::ModifierId;
use crate::values::action_start::ActionStart;
use crate::values::hit::Hit;

/// What the running call reads and queues, beside the units the view holds: its role, its
/// acting unit, what each capability's part of it holds, such as its params or the mode's state,
/// and the effects it queued. One frame serves the whole match, its buffers cleared and filled
/// again, so a call allocates none of them.
#[derive(Debug, Default)]
pub(crate) struct Frame {
    /// The running call's role; none between calls.
    role: Option<ScriptRole>,
    /// Its acting unit: a cast's caster, a modifier's source, the unit that thinks; none for the
    /// mode.
    acting: Option<StableId>,
    /// The action whose params it reads, at `rank`, and its modifier's.
    action: Option<ActionId>,
    rank: u8,
    modifier: Option<ModifierId>,
    /// The depth of the chain of combat events it runs in: 0 for a cast.
    depth: u8,
    /// The package whose names it means: 0 the mode's.
    package: u16,
    /// The hit a delivery's hook runs for.
    hit: Option<Hit>,
    /// How its action started.
    start: Option<ActionStart>,
    /// Whether it is a pure hook's, whose `ctx` only reads.
    pub(crate) pure: bool,
    /// The stable ids as the call takes them for the units it creates, and whether it took one.
    ids: IdAllocator,
    ids_taken: bool,
    /// The players' resources as the call sees them, when the match has them, and whether the
    /// call changed them.
    resources: Option<PlayerResources>,
    resources_written: bool,
    pub(crate) effects: Effects,
    /// What each capability reads and writes of a call besides the core's.
    parts: CallParts,
}

impl Frame {
    pub(crate) const fn role(&self) -> Option<ScriptRole> {
        self.role
    }

    pub(crate) const fn acting(&self) -> Option<StableId> {
        self.acting
    }

    pub(crate) const fn action(&self) -> Option<ActionId> {
        self.action
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

    pub(crate) const fn start(&self) -> Option<ActionStart> {
        self.start
    }

    /// Adds `part`, which every call readies, and which applies what a call wrote to it.
    pub(crate) fn add_part<P: CallPart>(&mut self, part: P) {
        self.parts.add(part);
    }

    /// The part of type `P`, when one was added.
    pub(crate) fn part<P: CallPart>(&self) -> Option<&P> {
        self.parts.get()
    }

    /// The part of type `P`, to change, when one was added.
    pub(crate) fn part_mut<P: CallPart>(&mut self) -> Option<&mut P> {
        self.parts.get_mut()
    }

    /// Starts `on_think` for `unit` in `world`.
    pub(crate) fn begin_think(&mut self, world: &World, unit: StableId) {
        let start = CallStart {
            acting: Some(unit),
            ..CallStart::mode(ScriptRole::Ai)
        };
        self.begin(world, start)
            .expect("a call with no params overflows none");
    }

    /// Starts a mode call, with the resources and stable ids as `world` holds them, for the call
    /// to read and write; `pure` for a hook whose `ctx` only reads.
    pub(crate) fn begin_mode(&mut self, world: &World, pure: bool) {
        self.begin(world, CallStart::mode(ScriptRole::Mode))
            .expect("a call with no params overflows none");
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
    pub(crate) const fn resources_mut(&mut self) -> Option<&mut PlayerResources> {
        self.resources_written = true;
        self.resources.as_mut()
    }

    /// Takes the stable id of a unit the call creates, which the unit spawns with once the call
    /// applies, so the call can hold it before then.
    pub(crate) const fn take_id(&mut self) -> StableId {
        self.ids_taken = true;
        self.ids.allocate()
    }

    /// Starts the call `start` in `world`, every part readied for it; a part that fails to ready,
    /// as a param that overflows at the call's rank, fails the call, and so does one at
    /// `ScriptLimits::CHAIN_DEPTH` of a chain of combat events, which the pass must end.
    pub(crate) fn begin(&mut self, world: &World, start: CallStart) -> Result<(), CallError> {
        if start.depth >= ScriptLimits::CHAIN_DEPTH {
            return Err(CallError::Api(ApiError::ChainTooDeep));
        }
        self.read_resources(world);
        self.ids.clone_from(world.resource::<IdAllocator>());
        self.ids_taken = false;
        let CallStart {
            role,
            acting,
            action,
            rank,
            modifier,
            package,
            depth,
            hit,
            start: action_start,
        } = start;
        self.role = Some(role);
        self.acting = acting;
        self.action = action;
        self.rank = rank;
        self.modifier = modifier;
        self.package = package;
        self.depth = depth;
        self.hit = hit;
        self.start = action_start;
        self.pure = false;
        self.effects.clear();
        self.parts.begin(world, &start)
    }

    /// Applies the effects the call that ran queued, in order, each itself, from the call's
    /// acting unit and its action at its rank, in tick `now`, after the ids it took, so a unit it
    /// creates spawns with the id it took. Then what the call wrote to each part applies, then
    /// to the players' resources.
    pub(crate) fn apply(&mut self, world: &mut World, now: Tick) {
        if self.ids_taken {
            world.resource_mut::<IdAllocator>().clone_from(&self.ids);
        }
        for at in 0..self.effects.order().len() {
            let apply = self.effects.order()[at];
            apply(world, self, now);
        }
        self.effects.clear();
        self.parts.apply(world);
        if let (true, Some(resources)) = (self.resources_written, &self.resources) {
            world
                .resource_mut::<PlayerResources>()
                .clone_from(resources);
        }
    }

    /// The running call's param `name`, of the part that holds its role's params.
    pub(crate) fn param_named(&self, name: &str) -> Option<Dynamic> {
        self.parts.param_named(self.role, name)
    }
}
