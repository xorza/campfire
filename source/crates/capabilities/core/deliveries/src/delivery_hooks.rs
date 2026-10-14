use bevy_ecs::entity::Entity;
use bevy_ecs::system::Local;
use bevy_ecs::world::World;
use campfire_actions::{ActionBook, ActionTarget, EffectListBook, ListsOf};
use campfire_script::{CallStart, Pool, ScriptValue, ScriptsWorldCrate};
use campfire_sim::{SimTick, SimWorld};
use campfire_units::{HitHandle, Owner};

use crate::Deliveries;
use crate::delivered::{Delivered, Reached};

/// The hooks of the tick's delivery hits and ends, and the despawn of the delivery units that
/// ended.
#[derive(Debug)]
pub(super) struct DeliveryHooks;

impl DeliveryHooks {
    /// Despawns the delivery units that ended this tick, in a match that runs no scripts, whose
    /// hits and ends run no hook or list.
    pub(super) fn end(world: &mut World, mut ended: Local<'_, Vec<Entity>>) {
        let mut deliveries = world.resource_mut::<Deliveries>();
        deliveries.delivered.clear();
        ended.clear();
        ended.append(&mut deliveries.ended);
        for &entity in &*ended {
            world.despawn(entity);
        }
    }

    /// Runs the hooks of the tick's hits and ends, then despawns the delivery units that ended.
    pub(super) fn deliver(
        world: &mut World,
        (mut due, mut ended): (Local<'_, Vec<Delivered>>, Local<'_, Vec<Entity>>),
    ) {
        let mut deliveries = world.resource_mut::<Deliveries>();
        due.clear();
        due.append(&mut deliveries.delivered);
        ended.clear();
        ended.append(&mut deliveries.ended);
        Self::run_hooks(world, &due);
        for &entity in &*ended {
            world.despawn(entity);
        }
    }

    /// Runs the hooks of `due`, in order: each with its action's params at its rank, its caster,
    /// `()` once gone, its target, `()` for an end, and its hit, from its caster's player's pool,
    /// or the think pool. The action's list for the hook queues first, to the unit reached; a hook
    /// its action's script does not define does not run, and its list applies alone. A launch's
    /// area runs its launch's list alone, and no hook. A failed call changes nothing, its list
    /// included, and is recorded.
    fn run_hooks(world: &mut World, due: &[Delivered]) {
        if due.is_empty() {
            return;
        }
        let now = world.resource::<SimTick>().start();
        world.script_batch(|batch| {
            let ctx = batch.ctx().clone();
            for delivered in due {
                let Delivered { by, reach, hit } = *delivered;
                let hook = reach.hook();
                let book = batch.world().resource::<ActionBook>();
                let action = book
                    .get(by.action)
                    .expect("a delivery's action is in the book");
                let script = action.hook(hook).filter(|_| by.launch.is_none());
                let package = action.package;
                let lists = batch.world().resource::<EffectListBook>();
                let of = by
                    .launch
                    .map_or(ListsOf::Action(by.action), ListsOf::Launch);
                if script.is_none() && lists.of(of, hook).is_empty() {
                    continue;
                }
                let view = ctx.view();
                let caster = view.unit_value(Some(by.source));
                let target = view.unit_value(reach.unit());
                let owner = batch
                    .world()
                    .entity_of(by.source)
                    .and_then(|entity| batch.world().get::<Owner>(entity))
                    .map(|owner| owner.slot());
                let start = CallStart {
                    hit: Some(hit),
                    start: by.start,
                    ..CallStart::cast(by.action, by.rank, by.source, package, now)
                };
                let pool = Pool::of(owner);
                batch.hook_call(start, hook, Some(by.source), |batch| {
                    EffectListBook::queue(
                        batch.world(),
                        of,
                        hook,
                        &mut ctx.frame(),
                        ctx.view(),
                        reach.unit().map_or(ActionTarget::None, ActionTarget::Unit),
                    )?;
                    let Some(script) = script else {
                        return Ok(());
                    };
                    let hit = ScriptValue::of(HitHandle::new(hit, view.clone()));
                    match reach {
                        Reached::Hit(_) => {
                            batch.call_hook(pool, script, hook, (ctx.clone(), caster, target, hit))
                        }
                        Reached::End => {
                            batch.call_hook(pool, script, hook, (ctx.clone(), caster, hit))
                        }
                    }
                });
            }
        });
    }
}
