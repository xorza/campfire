use bevy_ecs::world::World;
use campfire_common::{PlayerSlot, Tick};
use campfire_script::rhai::Dynamic;
use campfire_sim::{IdAllocator, StableId};

use crate::players::player_resources::PlayerResources;
use crate::players::players_world::{PlayersWorld, PlayersWorldCrate};
use crate::scripts::call_part::{CallPart, CallParts};
use crate::scripts::call_start::CallStart;
use crate::scripts::effects::Effects;
use crate::scripts::error::{ApiError, CallError};
use crate::scripts::script_limits::ScriptLimits;
use crate::scripts::script_role::ScriptRole;
use crate::units::action_id::ActionId;
use crate::units::forced_move::DashDelivery;
use crate::values::action_start::ActionStart;
use crate::values::hit::Hit;
use crate::values::rank::Rank;

/// What the running call reads and queues, beside the units the view holds: its role, its
/// acting unit, what each capability's part of it holds, such as its params or the mode's state,
/// and the effects it queued. One frame serves the whole match, its buffers cleared and filled
/// again, so a call allocates none of them.
#[derive(Debug, Default)]
pub(crate) struct Frame {
    /// How the running call started: its role, acting unit, action and the rest; none before
    /// the first call.
    call: Option<CallStart>,
    /// Whether it is a pure hook's, whose `ctx` only reads.
    pure: bool,
    /// The stable ids as the call takes them for the units it creates, and whether it took one.
    ids: IdAllocator,
    ids_taken: bool,
    /// The players' resources as the call sees them, by slot, none in a match with no mode, and
    /// whether the call changed them.
    resources: Vec<PlayerResources>,
    resources_written: bool,
    pub(crate) effects: Effects,
    /// What each capability reads and writes of a call besides the core's.
    parts: CallParts,
}

impl Frame {
    /// How the running call started.
    /// The tick the running call runs in, which its effects apply in.
    pub(crate) const fn now(&self) -> Tick {
        self.running().now
    }

    const fn running(&self) -> &CallStart {
        self.call.as_ref().expect("a call began")
    }

    pub(crate) fn role(&self) -> Option<ScriptRole> {
        self.call.map(|call| call.role)
    }

    pub(crate) fn acting(&self) -> Option<StableId> {
        self.call?.acting
    }

    pub(crate) fn action(&self) -> Option<ActionId> {
        self.call?.action
    }

    pub(crate) const fn rank(&self) -> Rank {
        self.running().rank
    }

    pub(crate) const fn depth(&self) -> u8 {
        self.running().depth
    }

    pub(crate) const fn package(&self) -> u16 {
        self.running().package
    }

    pub(crate) fn hit(&self) -> Option<Hit> {
        self.call?.hit
    }

    pub(crate) fn start(&self) -> Option<ActionStart> {
        self.call?.start
    }

    pub(crate) fn dash_delivers(&self) -> Option<DashDelivery> {
        self.call?.dash_delivers
    }

    /// Whether the running call is a pure hook's, whose `ctx` only reads.
    pub(crate) const fn pure(&self) -> bool {
        self.pure
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

    /// Starts `on_think` for `id` in `world`, in tick `now`.
    pub(crate) fn begin_think(&mut self, world: &World, id: StableId, now: Tick) {
        let start = CallStart {
            acting: Some(id),
            ..CallStart::mode(ScriptRole::Ai, now)
        };
        self.begin(world, start)
            .expect("a call with no params overflows none");
    }

    /// Starts a mode call in tick `now`, with the resources and stable ids as `world` holds them,
    /// for the call to read and write; `pure` for a hook whose `ctx` only reads.
    pub(crate) fn begin_mode(&mut self, world: &World, pure: bool, now: Tick) {
        self.begin(world, CallStart::mode(ScriptRole::Mode, now))
            .expect("a call with no params overflows none");
        self.pure = pure;
    }

    /// Reads the players' resources as `world` holds them, for the call to add to, each in the
    /// place it held at the last call, so a call allocates none.
    fn read_resources(&mut self, world: &World) {
        self.resources_written = false;
        let held = (0..).map_while(|slot| {
            let entity = world.player_entity(PlayerSlot::new(slot))?;
            world.get::<PlayerResources>(entity)
        });
        let mut players = 0;
        for (at, held) in held.enumerate() {
            match self.resources.get_mut(at) {
                Some(resources) => resources.clone_from(held),
                None => self.resources.push(held.clone()),
            }
            players = at + 1;
        }
        self.resources.truncate(players);
    }

    /// The resources of player `slot`, for the call to change; `None` for a player the session
    /// lacks, or in a match with no mode.
    pub(crate) fn resources_mut(&mut self, slot: PlayerSlot) -> Option<&mut PlayerResources> {
        self.resources_written = true;
        self.resources.get_mut(slot.index())
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
        self.call = Some(start);
        self.pure = false;
        self.effects.clear();
        self.parts.begin(world, &start)
    }

    /// Applies the effects the call that ran queued, in order, each itself, from the call's
    /// acting unit and its action at its rank, in the call's tick, after the ids it took, so a
    /// unit it creates spawns with the id it took. Then what the call wrote to each part applies,
    /// then to the players' resources.
    pub(crate) fn apply(&mut self, world: &mut World) {
        if self.ids_taken {
            world.resource_mut::<IdAllocator>().clone_from(&self.ids);
        }
        for at in 0..self.effects.order().len() {
            let apply = self.effects.order()[at];
            apply(world, self);
        }
        self.effects.clear();
        self.parts.apply(world);
        if !self.resources_written {
            return;
        }
        for (slot, resources) in (0..).map(PlayerSlot::new).zip(&self.resources) {
            let mut held = world
                .player_resources_mut(slot)
                .expect("a player the call read");
            if *held != *resources {
                held.clone_from(resources);
            }
        }
    }

    /// The running call's param `name`, of the part that holds its role's params.
    pub(crate) fn param(&self, name: &str) -> Option<Dynamic> {
        self.parts.param(self.role(), name)
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use campfire_common::Tick;

    use crate::scripts::call_start::CallStart;
    use crate::scripts::frame::Frame;
    use crate::scripts::script_role::ScriptRole;

    impl Frame {
        /// A frame whose mode call runs in tick `now`, as a test applies an effect in: it reads
        /// no world.
        pub(crate) fn in_tick(now: Tick) -> Frame {
            Frame {
                call: Some(CallStart::mode(ScriptRole::Mode, now)),
                ..Frame::default()
            }
        }
    }
}
