use bevy_ecs::query::Without;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{NonSend, Query, Res};
use bevy_ecs::world::{EntityRef, World};
use campfire_script::{ScriptHost, ScriptId};
use campfire_sim::{Position, SimSet, SimTick, StableId, StateRegistry, TickRate};

use crate::actions::action_book::{ActionBook, ActionId, ActionParts, Aim, Passive, RankValues};
use crate::actions::action_data::{ActionData, Range, Targeting};
use crate::actions::action_kind::ActionKind;
use crate::actions::action_slots::{ActionSlots, ActionTarget, InProgress};
use crate::actions::error::ActionError;
use crate::actions::purse::Purse;
use crate::actions::weapon::Weapon;
use crate::combat::CombatSet;
use crate::combat::dead::Dead;
use crate::combat::targets::Targets;
use crate::mode::player_resources::PlayerResources;
use crate::orders::OrdersSet;
use crate::scripts::ctx::Ctx;
use crate::stats::StatsSet;
use crate::stats::modifier_book::{Applier, ModifierBook};
use crate::stats::modifiers::Modifiers;
use crate::stats::param_sources::ParamSources;
use crate::stats::pools::Pools;
use crate::stats::stat::Stat;
use crate::stats::stat_book::StatBook;
use crate::units::block::Block;
use crate::units::body::Body;
use crate::units::owner::Owner;
use crate::units::script_view::{RowFill, SlotRow, View};
use crate::units::team::Team;
use crate::units::unit_tags::UnitTags;

pub(crate) mod action_book;
pub(crate) mod action_data;
pub(crate) mod action_kind;
pub(crate) mod action_slots;
pub(crate) mod delivery_data;
pub(crate) mod error;
pub(crate) mod purse;
pub(crate) mod slot_kind;
pub(crate) mod slot_kinds;
pub(crate) mod weapon;

/// The core's actions: every action a match loads, and the slots units hold them in.
#[derive(Debug)]
pub struct Actions;

impl Actions {
    /// Adds the actions to a match, with none loaded yet, and to the rows of its `view`.
    pub(crate) fn install(world: &mut World, registry: &mut StateRegistry, view: &View) {
        view.add_source(fill_row);
        world.insert_resource(ActionBook::default());
        registry.register_component::<ActionSlots>();
    }

    /// Adds the start of every action to `schedule`, in Act, and the passives of every action,
    /// as each stage that changes ranks or cooldowns ends: combat's to run, as every action's unit
    /// target is one combat finds.
    pub(crate) fn schedule(schedule: &mut Schedule) {
        schedule.add_systems((
            start_actions.in_set(SimSet::Act).in_set(CombatSet::Attack),
            hold_passives
                .in_set(SimSet::Inputs)
                .after(OrdersSet::Orders)
                .after(StatsSet::Expire),
            hold_passives
                .in_set(SimSet::Resolve)
                .after(CombatSet::Damage)
                .before(CombatSet::Die),
            hold_passives.in_set(SimSet::Vision),
        ));
    }

    /// Binds `action` to the unit type `name` it spawns, once the match's unit types load: a
    /// train's unit, or its delivery's projectile, one the package load checked.
    pub fn bind_spawn(world: &mut World, action: ActionId, name: &str) {
        let unit_type = world
            .non_send::<View>()
            .types_mut()
            .named(name)
            .expect("the load checked an action's unit type");
        world
            .resource_mut::<ActionBook>()
            .bind_spawn(action, unit_type);
    }

    /// Loads the action `name` of `package`, of `ranks` ranks, into the match, which the
    /// package load checked, with its compiled script exactly when its data names one: its
    /// capability fields at each rank, times in milliseconds as ticks at the match's rate,
    /// rounded up.
    pub fn load(
        world: &mut World,
        package: u16,
        name: &str,
        data: &ActionData,
        script: Option<ScriptId>,
        ranks: u8,
    ) -> Result<ActionId, ActionError> {
        let passive = data.passive_modifier.as_ref().map(|name| Passive {
            modifier: world
                .resource::<ModifierBook>()
                .find(package, name)
                .expect("the load checked the passive's modifier"),
            while_ready: data.passive_while_ready,
        });
        let rate = *world.resource::<TickRate>();
        let aim = match &data.targeting {
            Targeting::None => Aim::None,
            Targeting::Point => Aim::Point,
            Targeting::Direction => Aim::Direction,
            Targeting::Unit(filter) => Aim::Unit(
                world
                    .non_send::<View>()
                    .resolve_filter(filter)
                    .expect("the load checked the filter's tag"),
            ),
        };
        let view = world.non_send::<View>().clone();
        let ranks = RankValues::all(data, ranks, rate, |name| view.cost_target(name.as_str()))?;
        let stat = |stat: &Stat| view.stat_index(stat).expect("the load checked the stats");
        let weapon = match (&data.rate, &data.damage, &data.damage_kind) {
            (Some(rate), Some(damage), Some(kind)) => Some(Weapon {
                rate: stat(rate),
                damage: stat(damage),
                kind: view
                    .damage_kind(kind.as_str())
                    .expect("the load checked the damage kind"),
            }),
            _ => None,
        };
        let parts = ActionParts {
            passive,
            aim,
            ranks,
            weapon,
        };
        let host = world
            .remove_non_send::<ScriptHost>()
            .expect("units are installed");
        let id = world
            .resource_mut::<ActionBook>()
            .load(&host, package, data, script, parts);
        world.insert_non_send(host);
        let ctx = world.non_send::<Ctx>().clone();
        ctx.frame().add_params(id, &data.params, stat);
        let delivery = world
            .resource::<ActionBook>()
            .get(id)
            .and_then(|action| action.delivery);
        world.non_send::<View>().add_ability(name, delivery);
        Ok(id)
    }
}

/// Starts what each unit was ordered, in Act. An ordered cast that passes its checks, its target
/// within range, starts, and any other is dropped. With nothing under way, a unit attacks its
/// attack target with the first weapon whose filter selects it, once that weapon passes its
/// checks and the target is within its range. A unit its tags keep from an action's group keeps
/// its order: a cast it started goes back to it, and an attack in its windup stops. An attack
/// target that is no living enemy any more is dropped, with the attack on it.
fn start_actions(
    tick: Res<'_, SimTick>,
    book: Res<'_, ActionBook>,
    resources: Option<Res<'_, PlayerResources>>,
    targets: Targets<'_, '_>,
    mut units: Query<
        '_,
        '_,
        (
            &Position,
            &Team,
            &mut ActionSlots,
            Option<&Pools>,
            Option<&Owner>,
            Option<&Body>,
            Option<&UnitTags>,
        ),
        Without<Dead>,
    >,
) {
    let now = tick.start();
    for (&position, &team, mut slots, pools, owner, body, tags) in &mut units {
        let effects = UnitTags::effects_of(tags);
        let purse = Purse {
            pools,
            resources: resources.as_deref(),
            owner: owner.map(|owner| owner.slot()),
        };
        let radius = Body::radius_of(body);
        let living = |id| targets.living(id);
        let attitude = |other| targets.attitude(team, other);
        match slots.in_progress() {
            Some(underway) if underway.kind == ActionKind::Cast => {
                if effects.blocks(Block::Cast) {
                    if underway.resolves_at.is_some() {
                        slots.interrupt();
                    }
                    continue;
                }
                if underway.resolves_at.is_some() {
                    continue;
                }
                let started = book
                    .check(now, &slots, purse, underway, attitude, living)
                    .filter(|checked| checked.in_range(position, radius, &targets))
                    .map(|checked| (now.after(checked.values.windup), checked.target));
                match started {
                    Some((resolves_at, target)) => slots.start(resolves_at, target),
                    None => slots.stop(),
                }
            }
            Some(underway) if underway.kind == ActionKind::Train => {}
            Some(_) => {
                if effects.blocks(Block::Attack) {
                    slots.interrupt();
                } else if slots
                    .attack_target()
                    .is_none_or(|target| targets.enemy(team, target).is_none())
                {
                    slots.set_attack_target(None);
                }
            }
            None => {
                let Some(target) = slots.attack_target() else {
                    continue;
                };
                let Some(unit) = targets.enemy(team, target) else {
                    slots.set_attack_target(None);
                    continue;
                };
                if effects.blocks(Block::Attack) {
                    continue;
                }
                let selects = (targets.attitude(team, unit.team), unit.tags);
                let Some(slot) = book.weapon_for(&slots, Some(selects)) else {
                    continue;
                };
                let underway = InProgress {
                    slot,
                    kind: ActionKind::Attack,
                    target: ActionTarget::Unit(target),
                    resolves_at: None,
                };
                let started = book
                    .check(now, &slots, purse, underway, attitude, living)
                    .filter(|checked| checked.in_range(position, radius, &targets))
                    .map(|checked| now.after(checked.values.windup));
                if let Some(resolves_at) = started {
                    slots.start_attack(slot, resolves_at);
                }
            }
        }
    }
}

/// Keeps each unit's passives as its slots stand: the passive of each action with a rank, and
/// with `passive_while_ready` off cooldown, from the unit itself at the action's rank, applied
/// again when the rank changes; and none other. It runs as each tick starts, after the casts
/// resolve and the attacks strike, and after the mode's calls, which learn ranks. A passive's
/// params are the script's, so without the core's scripts, as on a client, it holds none.
fn hold_passives(
    actions: Res<'_, ActionBook>,
    book: Option<Res<'_, ModifierBook>>,
    stats: Option<Res<'_, StatBook>>,
    tick: Res<'_, SimTick>,
    ctx: Option<NonSend<'_, Ctx>>,
    sources: ParamSources<'_, '_>,
    mut units: Query<'_, '_, (&StableId, &ActionSlots, &mut Modifiers)>,
) {
    let (Some(book), Some(stats), Some(ctx)) = (book, stats, ctx) else {
        return;
    };
    let now = tick.start();
    for (&id, slots, mut modifiers) in &mut units {
        for slot in slots.iter() {
            let Some(passive) = actions.get(slot.action).and_then(|action| action.passive) else {
                continue;
            };
            let held = modifiers
                .get(passive.modifier, Some(id))
                .map(|instance| instance.rank);
            let holds = slot.rank > 0 && (!passive.while_ready || slot.ready_at <= now);
            if !holds {
                if held.is_some() {
                    modifiers.remove(passive.modifier, Some(id));
                }
                continue;
            }
            if held == Some(slot.rank) {
                continue;
            }
            let applier = Applier {
                source: Some(id),
                ability: Some(slot.action),
                rank: slot.rank,
                passive: true,
                held: false,
            };
            let frame = ctx.frame();
            let source = sources.get(id);
            let param = |name: &str| {
                let (ability, rank) = (Some(slot.action), slot.rank);
                frame.modifier_param(passive.modifier, ability, rank, name, source.as_ref())
            };
            if let Some(application) =
                book.application(passive.modifier, applier, None, now, &stats, param)
            {
                modifiers.apply(application);
            }
        }
    }
}

/// Fills a row of the script view with a unit's actions: each slot's rank and its action's
/// ranks, the attack target, and the range of its first weapon, which it has to attack at all.
fn fill_row(unit: &EntityRef<'_>, fill: &mut RowFill<'_>) {
    let Some(slots) = unit.get::<ActionSlots>() else {
        return;
    };
    let book = fill.world.resource::<ActionBook>();
    fill.row.target = slots.attack_target();
    fill.row.attack_range =
        book.weapon_for(slots, None)
            .and_then(|slot| match book.range(slots, slot) {
                Range::Meters(range) => Some(range),
                Range::Global => None,
            });
    let rows = slots.iter().map(|slot| {
        let action = book
            .get(slot.action)
            .expect("a slot's action is in the book");
        SlotRow {
            rank: slot.rank,
            ranks: u8::try_from(action.ranks.len()).expect("an action has few ranks"),
        }
    });
    fill.slotted(rows);
}
