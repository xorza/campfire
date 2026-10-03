use std::cell::RefCell;
use std::mem;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_script::rhai::Dynamic;
use campfire_sim::{EntityIndex, SimTick, StableId};

use crate::combat::combat_event::CombatEvent;
use crate::combat::damage::Damage;
use crate::combat::damage_handle::DamageHandle;
use crate::scripts::call_start::CallStart;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::CallError;
use crate::scripts::hook::Hook;
use crate::scripts::pool::Pool;
use crate::scripts::script_batch::ScriptBatch;
use crate::stats::instance::Instance;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::modifier_clocks::ModifierClocks;
use crate::stats::modifier_handle::HandleOf;
use crate::stats::modifiers::Modifiers;
use crate::stats::stats_call::StatsCall;
use crate::stats::stats_column::StatsColumn;
use crate::units::modifier_id::ModifierId;
use crate::units::owner::Owner;
use crate::units::tag_book::TagBook;

/// Runs modifier scripts' hooks for the combat events.
#[derive(Debug)]
pub(crate) struct ModifierHooks {
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
    pub(crate) const fn new(ctx: Ctx) -> ModifierHooks {
        ModifierHooks {
            ctx,
            heard: RefCell::new(Vec::new()),
        }
    }

    /// Answers `event` by the hooks of the modifiers that hear it, in the order they are kept.
    pub(crate) fn hear(&self, batch: &mut ScriptBatch<'_>, event: CombatEvent) {
        let unit = |id| {
            Some(
                self.ctx
                    .view()
                    .unit(id)
                    .map_or(Dynamic::UNIT, Dynamic::from),
            )
        };
        let damage = |damage: Damage| {
            let handle = DamageHandle::new(damage, self.ctx.view().clone());
            Some(Dynamic::from(handle))
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
                self.run(batch, attacker, Hook::OnAttack, 1, unit(target));
            }
            CombatEvent::AttackHit(hit) => {
                if let Some(attacker) = hit.source {
                    self.run(
                        batch,
                        attacker,
                        Hook::OnAttackHit,
                        next(hit.depth),
                        damage(hit),
                    );
                }
            }
            CombatEvent::DamageTaken(taken) => {
                let depth = next(taken.depth);
                self.run(
                    batch,
                    taken.target,
                    Hook::OnDamageTaken,
                    depth,
                    damage(taken),
                );
            }
            CombatEvent::Kill {
                killer,
                victim,
                depth,
            } => self.run(batch, killer, Hook::OnKill, next(depth), unit(victim)),
            CombatEvent::Takedown {
                unit: taker,
                victim,
                depth,
            } => self.run(batch, taker, Hook::OnTakedown, next(depth), unit(victim)),
        }
    }

    /// Runs `hook` of each modifier `carrier` holds whose script defines it, with `arg` after
    /// `ctx` and `m`, at chain depth `depth`.
    fn run(
        &self,
        batch: &mut ScriptBatch<'_>,
        carrier: StableId,
        hook: Hook,
        depth: u8,
        arg: Option<Dynamic>,
    ) {
        let mut heard = mem::take(&mut *self.heard.borrow_mut());
        heard.clear();
        let world = batch.world();
        let entity = world.resource::<EntityIndex>().get(carrier);
        if let Some(entity) = entity
            && let Some(modifiers) = world.get::<Modifiers>(entity)
        {
            let book = world.resource::<ModifierBook>();
            let defines = |id| book.get(id).hooks.contains(hook);
            let takes_effect = TagBook::effective(world, entity);
            heard.extend(
                modifiers
                    .iter()
                    .filter(|instance| defines(instance.id) && takes_effect(book.tags(instance.id)))
                    .map(|instance| Heard {
                        id: instance.id,
                        source: instance.source,
                    }),
            );
        }
        for &modifier in &heard {
            self.call(batch, carrier, modifier, hook, depth, arg.clone());
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
        arg: Option<Dynamic>,
    ) {
        let world = batch.world();
        let entity = world.resource::<EntityIndex>().get(carrier);
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
        let (ability, rank, package) = (instance.ability, instance.rank, entry.package);
        let pool = ModifierHooks::pool(world, heard.source);
        let start = CallStart {
            acting: heard.source,
            action: ability,
            rank,
            ..CallStart::hook(heard.id, package, depth)
        };
        let begun = self.ctx.frame().begin(world, start);
        if let Err(error) = begun {
            batch.record(Some(carrier), hook, error);
            return;
        }
        let handle = {
            let clocks = world
                .get::<ModifierClocks>(entity)
                .expect("a heard carrier");
            let mut frame = self.ctx.frame();
            let call = StatsCall::of_mut(&mut frame);
            let of = HandleOf {
                carrier,
                id: heard.id,
                source: heard.source,
                stacks: instance.stacks,
            };
            let handle =
                StatsColumn::held_handle(self.ctx.view(), call.spare(), of, clocks.state(at));
            call.handles.push(handle.clone());
            handle
        };
        let ctx = self.ctx.clone();
        let called = match arg {
            Some(arg) => batch.call(pool, script, hook, (ctx, handle, arg)),
            None => batch.call(pool, script, hook, (ctx, handle)),
        };
        match called {
            Ok(_) => {
                let now = batch.world().resource::<SimTick>().start();
                self.ctx.apply(batch.world(), now);
            }
            Err(error) => batch.record(Some(carrier), hook, CallError::from_script(error)),
        }
    }

    /// The pool a hook of a modifier from `source` draws from: its player's; the `think` pool when
    /// no player controls it, or it is gone; the mode's when the modifier has no source.
    fn pool(world: &World, source: Option<StableId>) -> Pool {
        let Some(source) = source else {
            return Pool::Mode;
        };
        let entity = world.resource::<EntityIndex>().get(source);
        entity
            .and_then(|entity| world.get::<Owner>(entity))
            .map_or(Pool::Think, |owner| Pool::Player(owner.slot()))
    }
}
