use bevy_ecs::world::World;
use campfire_common::{PlayerSlot, Tick};
use campfire_sim::{IdAllocator, StableId};
use campfire_values::{
    ActionId, ActionStart, Allegiance, DashDelivery, Hit, PackageIndex, Rank, Scalar,
};

use crate::scripts::budget_of::BudgetOf;
use crate::scripts::call_extension::{CallExtension, CallExtensions, CallParams, Commit};
use crate::scripts::call_start::{CallStart, Purity};
use crate::scripts::error::{ApiError, CallError};
use crate::scripts::frame::effects::{Effect, Effects};
use crate::scripts::script_limits::ScriptLimits;
use crate::scripts::script_role::ScriptRole;
use crate::scripts::script_value::ScriptValue;

pub(crate) mod effects;

/// What the running call reads and queues, beside the units the view holds: its role, its
/// acting unit, what each capability's part of it holds, such as its params or the mode's state,
/// and the effects it queued. One frame serves the whole match, its buffers cleared and filled
/// again, so a call allocates none of them.
#[derive(Debug, Default)]
pub struct Frame {
    /// How the running call started: its role, purity, acting unit, action and the rest; none
    /// before the first call.
    call: Option<CallStart>,
    /// The stable ids as the call takes them for the units it creates, and whether it took one.
    ids: IdAllocator,
    ids_taken: bool,
    effects: Effects,
    /// What each capability reads and writes of a call besides the core's.
    extensions: CallExtensions,
}

impl Frame {
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

    /// The budget the running call draws from, none for a pure call or none running.
    pub(crate) fn budget(&self) -> Option<BudgetOf> {
        self.call.as_ref().and_then(BudgetOf::of_call)
    }

    pub fn acting(&self) -> Option<StableId> {
        self.call?.acting
    }

    /// The allegiance of a cast's caster as its action began, which a list reads once the caster
    /// is gone.
    pub fn allegiance(&self) -> Option<Allegiance> {
        self.call?.allegiance
    }

    /// The player slot whose player pays for the running call, or whose bot thinks in it.
    pub fn payer(&self) -> Option<PlayerSlot> {
        self.call?.payer
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

    pub const fn package(&self) -> PackageIndex {
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
    pub fn is_pure(&self) -> bool {
        self.call.is_some_and(|call| call.purity == Purity::Reads)
    }

    /// Adds `extension`, which every call readies, and which commits what a call wrote to it.
    pub fn add_extension<P: CallExtension>(&mut self, extension: P) {
        self.extensions.add(extension);
    }

    /// Adds `extension` as `add_extension` does, the source of the params of its scope, which no
    /// extension holds yet.
    pub fn add_params<P: CallParams>(&mut self, extension: P) {
        self.extensions.add_params(extension);
    }

    /// The extension of type `P`, when one was added.
    pub fn extension<P: CallExtension>(&self) -> Option<&P> {
        self.extensions.get()
    }

    /// The extension of type `P`, to change, when one was added.
    pub fn extension_mut<P: CallExtension>(&mut self) -> Option<&mut P> {
        self.extensions.get_mut()
    }

    /// Takes the stable id of a unit the call creates, which the unit spawns with once the call
    /// applies, so the call can hold it before then.
    pub const fn take_id(&mut self) -> StableId {
        self.ids_taken = true;
        self.ids.allocate()
    }

    /// Starts a batch of calls in `world`, every part readied for it.
    pub(crate) fn begin_batch(&mut self, world: &World) {
        self.extensions.begin_batch(world);
    }

    /// Starts the call `start` in `world`, every part readied for it; a part that fails to ready,
    /// as a param that overflows at the call's rank, fails the call, and so does one at
    /// `ScriptLimits::CHAIN_DEPTH` of a chain of combat events, which the pass must end.
    pub(crate) fn begin(&mut self, world: &World, start: &CallStart) -> Result<(), CallError> {
        if start.depth >= ScriptLimits::CHAIN_DEPTH {
            return Err(CallError::Api(ApiError::ChainTooDeep));
        }
        self.ids.clone_from(world.resource::<IdAllocator>());
        self.ids_taken = false;
        self.call = Some(*start);
        self.effects.clear();
        self.extensions.begin(world, start)
    }

    /// Queues `effect`, which applies when the call's effects do.
    pub fn queue<E: Effect>(&mut self, effect: E) {
        self.effects.push(effect);
    }

    /// The effects of type `E` the call queued, in order.
    pub fn queued<E: Effect>(&self) -> impl Iterator<Item = &E> {
        self.effects.queued()
    }

    /// The effects of type `E` the call queued, in order, to change.
    pub fn queued_mut<E: Effect>(&mut self) -> impl Iterator<Item = &mut E> {
        self.effects.queued_mut()
    }

    /// Commits what the call that ran wrote and applies the effects it queued: the ids it took,
    /// so a unit it creates spawns with the id it took; the extensions that commit before the
    /// effects; the effects, in order, each itself, from the call's acting unit and its action
    /// at its rank, in the call's tick; then the other extensions.
    pub fn apply(&mut self, world: &mut World) {
        if self.ids_taken {
            world.resource_mut::<IdAllocator>().clone_from(&self.ids);
        }
        self.extensions.commit(world, Commit::BeforeEffects);
        let count = self.effects.order().len();
        for at in 0..count {
            let apply = self.effects.order()[at];
            apply(world, self);
        }
        debug_assert_eq!(
            self.effects.order().len(),
            count,
            "an effect queues no effect as it applies, which the clear would drop"
        );
        self.effects.clear();
        self.extensions.commit(world, Commit::AfterEffects);
    }

    /// The running call's param `name`, of the part that holds its role's scope of params; an
    /// error when no part holds them, or that part's call declares no such param.
    pub fn param(&self, name: &str) -> Result<ScriptValue, ApiError> {
        self.extensions.param(self.running().role.params(), name)
    }

    /// The running call's param at place `at` among its call's, of the part that holds its
    /// role's scope of params; none when no part holds them, or the call has no param there.
    pub fn param_at(&self, at: usize) -> Option<Scalar> {
        self.extensions.param_at(self.running().role.params(), at)
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
        pub fn in_tick(now: Tick) -> Self {
            Self {
                call: Some(CallStart::mode(ScriptRole::Mode, now)),
                ..Self::default()
            }
        }
    }
}
