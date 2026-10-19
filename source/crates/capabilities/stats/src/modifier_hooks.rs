use std::cell::RefCell;
use std::mem;

use bevy_ecs::entity::Entity;
use campfire_script::{CallStart, Ctx, Hook, HookSpec, ScriptBatch, View};
use campfire_sim::{SimTick, SimWorld, StableId};
use campfire_units::{Owner, TagEffect};
use campfire_values::ModifierId;

use crate::instance::Instance;
use crate::modifier_book::ModifierBook;
use crate::modifier_handle::{HandleOf, ModifierHandle};
use crate::modifiers::Modifiers;
use crate::modifiers::modifier_clocks::ModifierClocks;
use crate::stats_call::StatsCall;
use crate::stats_column::StatsColumn;

/// Runs modifier scripts' hooks: those of an interval, and those of the events another
/// capability reports to the modifiers its units hold, as combat's attacks, hits and kills.
#[derive(Debug)]
pub struct ModifierHooks {
    ctx: Ctx,
    /// The modifiers that hear the running event, kept between events.
    heard: RefCell<Vec<Heard>>,
}

/// A hook of the modifiers a unit holds, which their runner calls with `ctx`, the modifier's
/// handle, and what its event gives, as another capability reports the event.
pub trait ModifierHook: HookSpec {
    /// What its event gives after `ctx` and `m`, the same for each modifier that hears it.
    type Event: Clone;

    /// The call's arguments.
    fn args(ctx: Ctx, m: ModifierHandle, event: Self::Event) -> Self::Args;
}

/// A modifier instance that hears an event: its id and its source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Heard {
    pub(crate) id: ModifierId,
    pub(crate) source: Option<StableId>,
}

/// The instance a hook runs for, and its script state, as the call starts to see them.
#[derive(Debug)]
struct HeardInstance {
    entity: Entity,
    at: usize,
    instance: Instance,
}

impl ModifierHooks {
    pub const fn new(ctx: Ctx) -> Self {
        Self {
            ctx,
            heard: RefCell::new(Vec::new()),
        }
    }

    /// The script view its calls see.
    pub fn view(&self) -> &View {
        self.ctx.view()
    }

    /// Runs the hook `H` of each modifier `carrier` holds whose script defines it, with the event
    /// `event` gives after `ctx` and `m`, at chain depth `depth`. `event` runs only when one hears
    /// it: an event allocates its handle for no unit that has no hook for it.
    pub fn run<H: ModifierHook>(
        &self,
        batch: &mut ScriptBatch<'_>,
        carrier: StableId,
        depth: u8,
        event: impl FnOnce() -> H::Event,
    ) {
        let hook = Hook::of::<H>();
        let mut heard = mem::take(&mut *self.heard.borrow_mut());
        heard.clear();
        let world = batch.world();
        let entity = world.entity_of(carrier);
        if let Some(entity) = entity
            && let Some(modifiers) = world.get::<Modifiers>(entity)
        {
            let book = world.resource::<ModifierBook>();
            let defines = |id| book.get(id).hooks.contains(hook);
            let takes_effect = TagEffect::of(world, entity);
            heard.extend(
                modifiers
                    .iter()
                    .filter(|instance| {
                        defines(instance.id) && takes_effect.takes(book.tags(instance.id))
                    })
                    .map(|instance| Heard {
                        id: instance.id,
                        source: instance.source,
                    }),
            );
        }
        if !heard.is_empty() {
            let event = event();
            for &modifier in &heard {
                self.call::<H>(batch, carrier, modifier, depth, event.clone());
            }
        }
        *self.heard.borrow_mut() = heard;
    }

    /// Calls the hook `H` of the instance `heard` on `carrier`, if it still holds: in the pool of
    /// its source, with its params, its `m` handle, and `event`. Its effects apply when it
    /// returns; a call at `ScriptLimits::CHAIN_DEPTH` fails without running.
    pub(crate) fn call<H: ModifierHook>(
        &self,
        batch: &mut ScriptBatch<'_>,
        carrier: StableId,
        heard: Heard,
        depth: u8,
        event: H::Event,
    ) {
        let hook = Hook::of::<H>();
        let world = batch.world();
        let entity = world.entity_of(carrier);
        let found = entity.and_then(|entity| {
            let modifiers = world.get::<Modifiers>(entity)?;
            world.get::<ModifierClocks>(entity)?;
            let at = modifiers.position(heard.id, heard.source)?;
            let instance = *modifiers.get(heard.id, heard.source)?.instance;
            Some(HeardInstance {
                entity,
                at,
                instance,
            })
        });
        let Some(HeardInstance {
            entity,
            at,
            instance,
        }) = found
        else {
            return;
        };
        let entry = world.resource::<ModifierBook>().get(heard.id);
        if !entry.hooks.contains(hook) {
            return;
        }
        let script = entry
            .script
            .expect("a modifier whose script defines a hook has one");
        let (action, rank, package) = (instance.action, instance.rank, entry.package);
        let payer = heard.source.and_then(|source| {
            let entity = world.entity_of(source)?;
            Some(world.get::<Owner>(entity)?.slot())
        });
        let now = world.resource::<SimTick>().start();
        let start = CallStart {
            acting: heard.source,
            action,
            rank,
            payer,
            ..CallStart::hook(heard.id, package, depth, now)
        };
        let ctx = &self.ctx;
        batch.answer(start, hook, Some(carrier), |batch| {
            let handle = {
                let clocks = batch
                    .world()
                    .get::<ModifierClocks>(entity)
                    .expect("a heard carrier");
                let mut frame = ctx.frame();
                let call = StatsCall::of_mut(&mut frame);
                let of = HandleOf {
                    carrier,
                    id: heard.id,
                    source: heard.source,
                    stacks: instance.stacks,
                };
                StatsColumn::held_handle(ctx.view(), call, of, clocks.state(at))
            };
            let args = H::args(ctx.clone(), handle, event);
            batch.call::<H>(script, args).map(drop)
        });
    }
}
