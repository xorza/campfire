use bevy_ecs::world::World;
use campfire_math::Tick;
use campfire_script::rhai::Dynamic;
use campfire_sim::{Capability, IdAllocator, StableId};

use crate::players::player_resources::PlayerResources;
use crate::scripts::call_part::{CallPart, CallParts};
use crate::scripts::call_start::CallStart;
use crate::scripts::effects::{ApplyEffect, Effects};
use crate::scripts::error::CallError;
use crate::scripts::hook::ScriptRole;
use crate::units::action_id::ActionId;
use crate::units::modifier_id::ModifierId;
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
    /// Whether it is a pure hook's, whose `ctx` only reads.
    pub(crate) pure: bool,
    /// The stable ids as a mode call takes them for the units it spawns.
    pub(crate) ids: IdAllocator,
    /// The players' resources as the call sees them, when the match has them, and whether the
    /// call changed them.
    resources: Option<PlayerResources>,
    resources_written: bool,
    pub(crate) effects: Effects,
    /// How each capability applies its effects, by capability index; the capability table
    /// gives it at install.
    dispatch: [Option<ApplyEffect>; Capability::ALL.len()],
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

    /// Starts a cast of `ability` of `package` at `rank` by `caster` in `world`, or a hook of its
    /// delivery for `hit`; a part that fails to ready, as a param that overflows at that rank,
    /// fails the cast.
    pub(crate) fn begin_cast(
        &mut self,
        world: &World,
        ability: ActionId,
        rank: u8,
        caster: StableId,
        package: u16,
        hit: Option<Hit>,
    ) -> Result<(), CallError> {
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
        self.begin(world, start)
    }

    /// Starts a hook of `modifier` of `package` at chain depth `depth`, whose instance came from
    /// `source` by `ability` at `rank`; a part that fails to ready, as a param that overflows at
    /// that rank, fails the call.
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
        self.begin(world, start)
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
        self.ids.clone_from(world.resource::<IdAllocator>());
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

    /// Starts the call `start` in `world`, every part readied for it.
    fn begin(&mut self, world: &World, start: CallStart) -> Result<(), CallError> {
        self.read_resources(world);
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
        self.parts.begin(world, &start)
    }

    /// Applies the effects the call that ran queued, in order, each by its capability, from
    /// the call's acting unit and its ability at its rank, in tick `now`. Then what the call
    /// wrote to each part applies, then to the players' resources.
    pub(crate) fn apply(&mut self, world: &mut World, now: Tick) {
        for at in 0..self.effects.order().len() {
            let capability = self.effects.order()[at];
            let apply = self.dispatch[capability as usize]
                .expect("a capability that queues effects applies them");
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
