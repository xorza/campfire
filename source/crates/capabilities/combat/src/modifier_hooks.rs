use std::cell::RefCell;
use std::mem;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_script::{CallStart, Ctx, Hook, HookCall, Pool, ScriptBatch, ScriptValue};
use campfire_sim::{SimTick, SimWorld, StableId};
use campfire_stats::{
    HandleOf, Instance, ModifierBook, ModifierClocks, Modifiers, StatsCall, StatsColumn,
};
use campfire_units::{Owner, TagEffect};
use campfire_values::ModifierId;

use crate::combat_event::CombatEvent;
use crate::damage::Damage;
use crate::damage_handle::DamageHandle;

/// Runs modifier scripts' hooks for the combat events.
#[derive(Debug)]
pub struct ModifierHooks {
    ctx: Ctx,
    /// The modifiers that hear the running event, kept between events.
    heard: RefCell<Vec<Heard>>,
}

/// A modifier instance that hears an event: its id and its source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Heard {
    id: ModifierId,
    source: Option<StableId>,
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

    /// Answers `event` by the hooks of the modifiers that hear it, in the order they are kept.
    pub(crate) fn hear(&self, batch: &mut ScriptBatch<'_>, event: CombatEvent) {
        let unit = |id| Some(self.ctx.view().unit_value(Some(id)));
        let damage = |damage: Damage| {
            let handle = DamageHandle::new(damage, self.ctx.view().clone());
            Some(ScriptValue::of(handle))
        };
        let next = |depth: u8| depth.saturating_add(1);
        match event {
            CombatEvent::Interval {
                carrier,
                id,
                source,
            } => {
                let heard = Heard { id, source };
                self.call(batch, carrier, heard, Hook::OnInterval, 1, None);
            }
            CombatEvent::Attack { attacker, target } => {
                self.run(batch, attacker, Hook::OnAttack, 1, || unit(target));
            }
            CombatEvent::AttackHit(hit) => {
                if let Some(attacker) = hit.source {
                    self.run(batch, attacker, Hook::OnAttackHit, next(hit.depth), || {
                        damage(hit)
                    });
                }
            }
            CombatEvent::DamageTaken(taken) => {
                let depth = next(taken.depth);
                self.run(batch, taken.target, Hook::OnDamageTaken, depth, || {
                    damage(taken)
                });
            }
            CombatEvent::Kill {
                killer,
                victim,
                depth,
            } => self.run(batch, killer, Hook::OnKill, next(depth), || unit(victim)),
            CombatEvent::Takedown {
                unit: taker,
                victim,
                depth,
            } => self.run(batch, taker, Hook::OnTakedown, next(depth), || unit(victim)),
        }
    }

    /// Runs `hook` of each modifier `carrier` holds whose script defines it, with what `arg`
    /// gives after `ctx` and `m`, at chain depth `depth`. `arg` runs only when one hears it: an
    /// event allocates its handle for no unit that has no hook for it.
    fn run(
        &self,
        batch: &mut ScriptBatch<'_>,
        carrier: StableId,
        hook: Hook,
        depth: u8,
        arg: impl FnOnce() -> Option<ScriptValue>,
    ) {
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
            let arg = arg();
            for &modifier in &heard {
                self.call(batch, carrier, modifier, hook, depth, arg.clone());
            }
        }
        *self.heard.borrow_mut() = heard;
    }

    /// Calls `hook` of the instance `heard` on `carrier`, if it still holds: in the pool of its
    /// source, with its params, its `m` handle, and `arg`. Its effects apply when it returns; a
    /// call at `ScriptLimits::CHAIN_DEPTH` fails without running.
    fn call(
        &self,
        batch: &mut ScriptBatch<'_>,
        carrier: StableId,
        heard: Heard,
        hook: Hook,
        depth: u8,
        arg: Option<ScriptValue>,
    ) {
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
        let pool = Self::pool(world, heard.source);
        let now = world.resource::<SimTick>().start();
        let start = CallStart {
            acting: heard.source,
            action,
            rank,
            ..CallStart::hook(heard.id, package, depth, now)
        };
        let ctx = &self.ctx;
        batch.hook_call(start, hook, Some(carrier), |batch| {
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
            if let Some(arg) = arg {
                let args = (ctx.clone(), handle, arg);
                batch.call(pool, HookCall { script, hook, args }).map(drop)
            } else {
                let args = (ctx.clone(), handle);
                batch.call(pool, HookCall { script, hook, args }).map(drop)
            }
        });
    }

    /// The pool a hook of a modifier from `source` draws from: its player's; the `think` pool when
    /// no player controls it, or it is gone; the mode's when the modifier has no source.
    fn pool(world: &World, source: Option<StableId>) -> Pool {
        let Some(source) = source else {
            return Pool::Mode;
        };
        let entity = world.entity_of(source);
        let owner = entity.and_then(|entity| world.get::<Owner>(entity));
        Pool::of(owner.map(|owner| owner.slot()))
    }
}
