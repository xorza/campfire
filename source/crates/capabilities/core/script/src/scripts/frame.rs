use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_sim::{IdAllocator, StableId};
use campfire_values::{ActionId, ActionStart, DashDelivery, Hit, Rank};

use crate::scripts::call_part::{CallParams, CallPart, CallParts, Commit};
use crate::scripts::call_start::CallStart;
use crate::scripts::effects::Effects;
use crate::scripts::error::{ApiError, CallError};
use crate::scripts::script_limits::ScriptLimits;
use crate::scripts::script_role::ScriptRole;
use crate::scripts::script_value::ScriptValue;

/// What the running call reads and queues, beside the units the view holds: its role, its
/// acting unit, what each capability's part of it holds, such as its params or the mode's state,
/// and the effects it queued. One frame serves the whole match, its buffers cleared and filled
/// again, so a call allocates none of them.
#[derive(Debug, Default)]
pub struct Frame {
    /// How the running call started: its role, acting unit, action and the rest; none before
    /// the first call.
    call: Option<CallStart>,
    /// Whether it is a pure hook's, whose `ctx` only reads.
    pure: bool,
    /// The stable ids as the call takes them for the units it creates, and whether it took one.
    ids: IdAllocator,
    ids_taken: bool,
    pub effects: Effects,
    /// What each capability reads and writes of a call besides the core's.
    parts: CallParts,
}

impl Frame {
    /// How the running call started.
    /// The tick the running call runs in, which its effects apply in.
    pub const fn now(&self) -> Tick {
        self.running().now
    }

    const fn running(&self) -> &CallStart {
        self.call.as_ref().expect("a call began")
    }

    pub fn role(&self) -> Option<ScriptRole> {
        self.call.map(|call| call.role)
    }

    pub fn acting(&self) -> Option<StableId> {
        self.call?.acting
    }

    pub fn action(&self) -> Option<ActionId> {
        self.call?.action
    }

    pub const fn rank(&self) -> Rank {
        self.running().rank
    }

    pub const fn depth(&self) -> u8 {
        self.running().depth
    }

    pub const fn package(&self) -> u16 {
        self.running().package
    }

    pub fn hit(&self) -> Option<Hit> {
        self.call?.hit
    }

    pub fn start(&self) -> Option<ActionStart> {
        self.call?.start
    }

    pub fn dash_delivers(&self) -> Option<DashDelivery> {
        self.call?.dash_delivers
    }

    /// Whether the running call is a pure hook's, whose `ctx` only reads.
    pub const fn pure(&self) -> bool {
        self.pure
    }

    /// Adds `part`, which every call readies, and which commits what a call wrote to it.
    pub fn add_part<P: CallPart>(&mut self, part: P) {
        self.parts.add(part);
    }

    /// Adds `part` as `add_part` does, the source of the params of its scope, which no part
    /// holds yet.
    pub fn add_params<P: CallParams>(&mut self, part: P) {
        self.parts.add_params(part);
    }

    /// The part of type `P`, when one was added.
    pub fn part<P: CallPart>(&self) -> Option<&P> {
        self.parts.get()
    }

    /// The part of type `P`, to change, when one was added.
    pub fn part_mut<P: CallPart>(&mut self) -> Option<&mut P> {
        self.parts.get_mut()
    }

    /// Starts `on_think` for `id` in `world`, in tick `now`.
    pub fn begin_think(&mut self, world: &World, id: StableId, now: Tick) {
        let start = CallStart {
            acting: Some(id),
            ..CallStart::mode(ScriptRole::Ai, now)
        };
        self.begin(world, start)
            .expect("a call with no params overflows none");
    }

    /// Starts a mode call in tick `now`, with the stable ids as `world` holds them,
    /// for the call to read and write; `pure` for a hook whose `ctx` only reads.
    pub fn begin_mode(&mut self, world: &World, pure: bool, now: Tick) {
        self.begin(world, CallStart::mode(ScriptRole::Mode, now))
            .expect("a call with no params overflows none");
        self.pure = pure;
    }

    /// Takes the stable id of a unit the call creates, which the unit spawns with once the call
    /// applies, so the call can hold it before then.
    pub const fn take_id(&mut self) -> StableId {
        self.ids_taken = true;
        self.ids.allocate()
    }

    /// Starts the call `start` in `world`, every part readied for it; a part that fails to ready,
    /// as a param that overflows at the call's rank, fails the call, and so does one at
    /// `ScriptLimits::CHAIN_DEPTH` of a chain of combat events, which the pass must end.
    pub fn begin(&mut self, world: &World, start: CallStart) -> Result<(), CallError> {
        if start.depth >= ScriptLimits::CHAIN_DEPTH {
            return Err(CallError::Api(ApiError::ChainTooDeep));
        }
        self.ids.clone_from(world.resource::<IdAllocator>());
        self.ids_taken = false;
        self.call = Some(start);
        self.pure = false;
        self.effects.clear();
        self.parts.begin(world, &start)
    }

    /// Commits what the call that ran wrote and applies the effects it queued: the ids it took,
    /// so a unit it creates spawns with the id it took; the parts that commit before the
    /// effects; the effects, in order, each itself, from the call's acting unit and its action
    /// at its rank, in the call's tick; then the other parts.
    pub fn apply(&mut self, world: &mut World) {
        if self.ids_taken {
            world.resource_mut::<IdAllocator>().clone_from(&self.ids);
        }
        self.parts.commit(world, Commit::BeforeEffects);
        for at in 0..self.effects.order().len() {
            let apply = self.effects.order()[at];
            apply(world, self);
        }
        self.effects.clear();
        self.parts.commit(world, Commit::AfterEffects);
    }

    /// The running call's param `name`, of the part that holds its role's scope of params; an
    /// error when no part holds them, or that part's call declares no such param.
    pub fn param(&self, name: &str) -> Result<ScriptValue, ApiError> {
        self.parts.param(self.running().role.params(), name)
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_common::Tick;

    use crate::scripts::call_start::CallStart;
    use crate::scripts::frame::Frame;
    use crate::scripts::script_role::ScriptRole;

    impl Frame {
        /// A frame whose mode call runs in tick `now`, as a test applies an effect in: it reads
        /// no world.
        pub fn in_tick(now: Tick) -> Frame {
            Frame {
                call: Some(CallStart::mode(ScriptRole::Mode, now)),
                ..Frame::default()
            }
        }
    }
}
