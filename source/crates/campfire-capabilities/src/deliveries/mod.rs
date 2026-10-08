use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::system::Local;
use bevy_ecs::world::World;
use campfire_math::Vec3;
use campfire_script::rhai::Dynamic;
use campfire_sim::{EntityIndex, Position, SimSet, SimTick, StableId};

use crate::actions::action_book::ActionBook;
use crate::actions::action_target::ActionTarget;
use crate::actions::effect_lists::EffectLists;
use crate::actions::lists_of::ListsOf;
use crate::combat::CombatSet;
use crate::deliveries::delivered::{Delivered, Reached};
use crate::deliveries::deliverers::Deliverers;
use crate::deliveries::delivering::Delivering;
use crate::scripts::call_start::CallStart;
use crate::scripts::ctx::Ctx;
use crate::scripts::pool::Pool;
use crate::scripts::script_batch::ScriptBatch;
use crate::units::forced_move::DashDelivery;
use crate::units::hit_handle::HitHandle;
use crate::units::owner::Owner;
use crate::values::hit::Hit;

pub(crate) mod delivered;
pub(crate) mod deliverers;
pub(crate) mod deliveries_api;
pub(crate) mod delivering;
pub(crate) mod delivery_spawner;

/// The hits and ends of this tick's deliveries whose action's hooks run, in the order they
/// happened; and the delivery units that ended, which despawn once the hooks ran, so a hook still
/// reads its delivery. Not state: it empties within the tick.
#[derive(Resource, Debug, Default)]
pub(crate) struct Deliveries {
    pub(crate) delivered: Vec<Delivered>,
    pub(crate) ended: Vec<Entity>,
}

/// The systems that move the deliveries of actions and find what they reach, in Hit before
/// attacks strike, in this order; their hooks run after them.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum DeliverySet {
    /// Projectiles fly.
    Fly,
    /// Areas trigger and end.
    Trigger,
}

impl Deliveries {
    /// Records the end of a dash that delivers `dash`, aimed at `target` if a unit, whose unit
    /// stands at `place` after a last step in `direction`: an end with no delivery unit, after
    /// the meters the dash went, whose hooks are its action's.
    pub(crate) fn end_dash(
        &mut self,
        dash: DashDelivery,
        target: Option<StableId>,
        place: Position,
        direction: Option<Vec3>,
    ) {
        let by = Delivering {
            source: dash.source,
            action: dash.action,
            rank: dash.rank,
            start: dash.start,
            launch: None,
        };
        let hit = Hit {
            delivery: None,
            target,
            pos: place,
            distance: dash.dashed,
            direction,
        };
        self.delivered.push(Delivered {
            by,
            reach: Reached::End,
            hit,
        });
    }

    /// Adds the deliveries' hooks to a match, once for the capabilities that deliver: after the
    /// `DeliverySet`s, before attacks pay their toggles and strike.
    pub(crate) fn install(world: &mut World, schedule: &mut Schedule) {
        if world.contains_resource::<Deliveries>() {
            return;
        }
        world.insert_resource(Deliveries::default());
        world.insert_resource(Deliverers::default());
        schedule.configure_sets(
            (DeliverySet::Fly, DeliverySet::Trigger)
                .chain()
                .in_set(SimSet::Hit)
                .before(CombatSet::Pay),
        );
        schedule.add_systems(
            deliver
                .in_set(SimSet::Hit)
                .after(DeliverySet::Trigger)
                .before(CombatSet::Pay),
        );
    }
}

/// Runs the hooks of the tick's hits and ends, then despawns the delivery units that ended.
fn deliver(
    world: &mut World,
    (mut due, mut ended): (Local<'_, Vec<Delivered>>, Local<'_, Vec<Entity>>),
) {
    let mut deliveries = world.resource_mut::<Deliveries>();
    due.clear();
    due.append(&mut deliveries.delivered);
    ended.clear();
    ended.append(&mut deliveries.ended);
    run_hooks(world, &due);
    for &entity in &*ended {
        world.despawn(entity);
    }
}

/// Runs the hooks of `due`, in order: each with its action's params at its rank, its caster,
/// `()` once gone, its target, `()` for an end, and its hit, from its caster's player's pool, or
/// the think pool. The action's list for the hook queues first, to the unit reached; a hook its
/// action's script does not define does not run, and its list applies alone. A launch's area
/// runs its launch's list alone, and no hook. A failed call changes nothing, its list included,
/// and is recorded.
fn run_hooks(world: &mut World, due: &[Delivered]) {
    if due.is_empty() {
        return;
    }
    let Some(ctx) = world.get_non_send::<Ctx>().cloned() else {
        return;
    };
    let now = world.resource::<SimTick>().start();
    ScriptBatch::run(world, ctx.view(), |batch| {
        for delivered in due {
            let Delivered { by, reach, hit } = *delivered;
            let hook = reach.hook();
            let book = batch.world().resource::<ActionBook>();
            let action = book
                .get(by.action)
                .expect("a delivery's action is in the book");
            let script = action.hook(hook).filter(|_| by.launch.is_none());
            let package = action.package;
            let lists = batch.world().resource::<EffectLists>();
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
                .resource::<EntityIndex>()
                .get(by.source)
                .and_then(|entity| batch.world().get::<Owner>(entity))
                .map(|owner| owner.slot());
            let start = CallStart {
                hit: Some(hit),
                start: by.start,
                ..CallStart::cast(by.action, by.rank, by.source, package)
            };
            let pool = Pool::of(owner);
            batch.hook_call(&ctx, now, start, hook, Some(by.source), |batch| {
                EffectLists::queue(
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
                let hit = Dynamic::from(HitHandle::new(hit, view.clone()));
                match reach {
                    Reached::Hit(_) => {
                        batch.call_hook(pool, script, hook, (ctx.clone(), caster, target, hit))
                    }
                    Reached::End => batch.call_hook(pool, script, hook, (ctx.clone(), caster, hit)),
                }
            });
        }
    });
}
