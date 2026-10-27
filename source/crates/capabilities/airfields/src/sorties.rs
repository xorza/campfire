use std::num::{NonZeroI64, NonZeroU64};

use bevy_ecs::change_detection::{DetectChangesMut, Mut};
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Added, Has, With, Without};
use bevy_ecs::system::{Commands, Local, Query, Res};
use bevy_ecs::world::World;
use campfire_actions::{ActionProgress, ActionSlots, ProgressIndexes};
use campfire_combat::{ClipRefill, ClipShots, Weapon};
use campfire_common::{Tick, Ticks};
use campfire_declarations::ById;
use campfire_math::{Num, Rounding, Vec3};
use campfire_navigation::FlightData;
use campfire_production::{FramePoint, HeldPlaces, MadeAt, ProducerSpec};
use campfire_sim::{EntityIndex, Keyed, MatchRate, Ordered, Position, SimTick, StableId};
use campfire_units::{
    DashTo, Dead, Destination, Facing, FlightGoal, ForcedMove, MoveStep, StatusTags,
};
use campfire_values::{ActionId, Team, UnitType};

use crate::Airfields;
use crate::airfield_data::AirfieldData;
use crate::runways::Runways;
use crate::sortie::Sortie;
use crate::sortie_data::SortieSpec;

/// The sorties of the units airfields park: each one's way from its place, along its runway, into
/// the air, and back.
#[derive(Debug)]
pub(crate) struct Sorties;

/// The parts of an airfield a sortie reads and writes: its id, place, facing, type, team, runways
/// and places.
type Fields<'w, 's> = Query<
    'w,
    's,
    (
        (
            &'static StableId,
            &'static Position,
            Option<&'static Facing>,
        ),
        (&'static UnitType, &'static Team),
        (&'static mut Runways, &'static mut HeldPlaces),
    ),
    (Without<Dead>, Without<Sortie>),
>;

/// The parts of a unit of a sortie its sortie reads and writes.
type Sortied<'w, 's> = Query<
    'w,
    's,
    (
        (Entity, &'static StableId, &'static UnitType, &'static Team),
        (&'static Position, &'static MoveStep),
        (
            &'static mut Sortie,
            &'static mut MadeAt,
            &'static mut Facing,
            &'static mut FlightGoal,
            &'static mut StatusTags,
        ),
        (
            &'static mut Destination,
            Option<&'static mut ActionProgress>,
            Has<ForcedMove>,
        ),
        (Option<&'static ActionSlots>, Option<&'static mut ClipShots>),
    ),
    (Without<Dead>, Without<Runways>),
>;

/// The units of a sortie that died this tick, with where they were made.
type DeadSorties<'w, 's> =
    Query<'w, 's, (&'static StableId, &'static MadeAt), (Added<Dead>, With<Sortie>)>;

/// The books a sortie reads: the airfields', the sorties', the producers' places, the flights and
/// the weapons.
type SortieBooks<'w> = (
    Res<'w, ById<UnitType, AirfieldData>>,
    Res<'w, ById<UnitType, SortieSpec>>,
    Res<'w, ById<UnitType, ProducerSpec>>,
    Res<'w, ById<UnitType, FlightData>>,
    Option<Res<'w, ById<ActionId, Weapon>>>,
);

/// Where a sortie's airfield stands: its place and facing, and its airfield's own points.
#[derive(Debug, Clone, Copy)]
struct Field {
    at: Vec3,
    angle: Num,
}

impl Field {
    /// `frame`'s point in the world.
    fn point(self, frame: FramePoint) -> Vec3 {
        frame.placed(self.at, self.angle)
    }
}

impl Sorties {
    /// Parks a unit of a sortie its airfield made at one of its places, as it spawns: on the
    /// ground, grounded, until it is ordered away (jets.md, A jet holds a place). One made at a
    /// place of a producer of no runways flies from there, as nothing would take it off.
    pub(crate) fn park(world: &mut World, entity: Entity) {
        let Some(&MadeAt { producer, .. }) = world.get::<MadeAt>(entity) else {
            return;
        };
        let producer = world.resource::<EntityIndex>().get(producer);
        if !producer.is_some_and(|producer| world.entity(producer).contains::<Runways>()) {
            return;
        }
        let mut unit = world.entity_mut(entity);
        let Some(mut sortie) = unit.get_mut::<Sortie>() else {
            return;
        };
        *sortie = Sortie::Parked;
        if let Some(mut goal) = unit.get_mut::<FlightGoal>() {
            *goal = FlightGoal::Ground;
        }
        if let Some(mut status) = unit.get_mut::<StatusTags>() {
            *status = status.turned(Airfields::GROUNDED, true);
        }
    }

    /// Steps each unit of a sortie, by stable id, before the units move (jets.md, A jet takes off
    /// and lands on its runway; jets.md, A jet goes home). One ordered away from its place waits
    /// for its runway, holds it, taxis to its prep point and its start at its taxi speed, waits its
    /// pause, rolls to the runway's end at its speed, lets the runway go and climbs, its orders its
    /// own. In the air, one whose clips that refill at home are all empty, or that stood idle its
    /// idle time, drops its orders and flies home to its runway's approach, past the runway's end
    /// as far as it flies while it sinks from its height; it circles there until it holds the
    /// runway, sinks to its end, rolls to its start, lets it go, taxis back to its place and
    /// reloads its clips, all in the longest time a clip's reload takes for the share it lacks, its
    /// shots back as the time goes. One whose airfield is gone takes the nearest airfield of its
    /// team with a free place, and with none flies on as it is; one on the ground of a gone
    /// airfield stays. It is grounded, its walk and its attacks blocked, from its runway's end to
    /// its end.
    pub(crate) fn step(
        (tick, rate, index): (Res<'_, SimTick>, Res<'_, MatchRate>, Res<'_, EntityIndex>),
        books: SortieBooks<'_>,
        mut units: Sortied<'_, '_>,
        mut fields: Fields<'_, '_>,
        living: Query<'_, '_, (), (With<HeldPlaces>, Without<Dead>)>,
        (mut indexes, mut commands, mut order): (
            ProgressIndexes<'_>,
            Commands<'_, '_>,
            Local<'_, Ordered>,
        ),
    ) {
        let now = tick.start();
        let (airfields, sorties, producers, flights, weapons) = books;
        let hz = NonZeroI64::from(rate.hz()).get();
        let ordered = units
            .iter()
            .map(|((entity, &id, ..), ..)| Keyed { id, entity });
        for &Keyed { entity, .. } in order.sort(ordered) {
            let (
                (entity, &id, &unit_type, &team),
                (&at, step),
                (mut sortie, mut made, mut facing, mut goal, mut status),
                (mut destination, mut progress, moving),
                (slots, mut clips),
            ) = units.get_mut(entity).expect("a sortie in the order");
            let at = at.get();
            let spec = sorties
                .get(unit_type)
                .expect("a sortie's type has its sortie");
            let home_clips = || {
                let clips = clips.as_deref().zip(slots).zip(weapons.as_deref());
                clips
                    .into_iter()
                    .flat_map(|((clips, slots), weapons)| clips.at_home(slots, weapons))
            };
            let producer = index.get(made.producer);
            let mut home = producer.filter(|&field| fields.contains(field));
            // A unit of a gone airfield that flies takes another of its team's with a free place;
            // one of a living producer of no runways keeps its place there.
            let gone = producer.is_none_or(|producer| !living.contains(producer));
            if home.is_none() && gone && sortie.flies() {
                let free =
                    fields
                        .iter()
                        .filter_map(|((&field, &place, _), (_, &of), (_, held))| {
                            let free = held.free().filter(|_| of == team)?;
                            Some((place.get().ground_offset(at).length(), field, free))
                        });
                if let Some((_, field, place)) = free.min() {
                    let entity = index.get(field).expect("a living airfield is indexed");
                    let (_, _, (_, mut held)) = fields.get_mut(entity).expect("an airfield");
                    held.hold_for(place, id);
                    made.set_if_neq(MadeAt {
                        producer: field,
                        place,
                    });
                    home = Some(entity);
                }
            }
            let Some(home) = home else {
                if matches!(*sortie, Sortie::Home | Sortie::Approach) {
                    sortie.set_if_neq(Sortie::FLYING);
                }
                continue;
            };
            let ((_, &field_at, field_facing), (&field_type, _), (mut runways, _)) =
                fields.get_mut(home).expect("a sortie's airfield");
            let field = Field {
                at: field_at.get(),
                angle: field_facing.map_or(Num::ZERO, |facing| facing.degrees()),
            };
            let airfield = airfields
                .get(field_type)
                .expect("an airfield's type has its airfield");
            let place = airfield.places[usize::from(made.place)];
            let runway = airfield.runways[usize::from(place.runway)];
            let parked = producers
                .get(field_type)
                .expect("an airfield produces")
                .places[usize::from(made.place)];
            let ordered = destination.get().is_some()
                || progress
                    .as_ref()
                    .is_some_and(|progress| progress.attack_target().is_some());
            let mut dash = |facing: &mut Mut<'_, Facing>, to: Vec3, step: Num| {
                Self::dash(&mut commands, entity, facing, at, to, step);
            };
            let next = match *sortie {
                Sortie::Flying { idle_since } => {
                    let idle_since = if ordered {
                        None
                    } else {
                        idle_since.or(Some(now))
                    };
                    let mut clips = home_clips().peekable();
                    let empty = clips.peek().is_some()
                        && clips.all(|home| home.fired >= home.clip.shots.get());
                    let idled = spec
                        .idle
                        .zip(idle_since)
                        .is_some_and(|(idle, since)| now >= since.after(idle));
                    if empty || idled {
                        if let Some(progress) = progress.as_mut() {
                            progress.set_attack_target(None, &mut indexes.of(id, entity));
                        }
                        let flight = flights.get(unit_type).expect("a sortie's type flies");
                        let approach =
                            Self::approach(field, runway.start, runway.end, flight, *step, hz);
                        Destination::route_to(&mut destination, Some(approach));
                        Sortie::Home
                    } else {
                        Sortie::Flying { idle_since }
                    }
                }
                Sortie::Home => {
                    // On its way home it heeds no order, and arrives at its approach as its walk
                    // there ends.
                    let flight = flights.get(unit_type).expect("a sortie's type flies");
                    let approach =
                        Self::approach(field, runway.start, runway.end, flight, *step, hz);
                    match destination.get() {
                        None => Sortie::Approach,
                        Some(to) if to == approach => Sortie::Home,
                        Some(_) => {
                            Destination::route_to(&mut destination, Some(approach));
                            Sortie::Home
                        }
                    }
                }
                Sortie::Approach if runways.take(place.runway, id) => {
                    goal.set_if_neq(FlightGoal::Ground);
                    dash(&mut facing, field.point(runway.end), step.get());
                    Sortie::Descent
                }
                Sortie::Descent if !moving => {
                    dash(&mut facing, field.point(runway.start), step.get());
                    Sortie::Landing
                }
                Sortie::Landing if !moving => {
                    runways.release(place.runway, id);
                    dash(&mut facing, field.point(place.prep), spec.taxi_step);
                    Sortie::BackToPrep
                }
                Sortie::BackToPrep if !moving => {
                    dash(&mut facing, field.point(parked.at), spec.taxi_step);
                    Sortie::BackToPlace
                }
                Sortie::BackToPlace if !moving => {
                    facing.set_if_neq(Facing::of(field.angle + parked.angle));
                    let reload = home_clips()
                        .map(|home| {
                            Self::share(home.clip.reload, home.fired, home.clip.shots.get())
                        })
                        .max()
                        .unwrap_or(Ticks::ZERO);
                    if reload == Ticks::ZERO {
                        Sortie::Parked
                    } else {
                        Sortie::Reload {
                            from: now,
                            until: now.after(reload),
                        }
                    }
                }
                Sortie::Reload { from, until } => {
                    if let (Some(clips), Some(slots), Some(weapons)) =
                        (clips.as_deref_mut(), slots, weapons.as_deref())
                    {
                        Self::reload(clips, slots, weapons, now, from, until);
                    }
                    if now >= until {
                        Sortie::Parked
                    } else {
                        *sortie
                    }
                }
                Sortie::Parked if ordered => Sortie::Waiting,
                Sortie::Waiting if runways.take(place.runway, id) => {
                    dash(&mut facing, field.point(place.prep), spec.taxi_step);
                    Sortie::ToPrep
                }
                Sortie::ToPrep if !moving => {
                    dash(&mut facing, field.point(runway.start), spec.taxi_step);
                    Sortie::ToStart
                }
                Sortie::ToStart if !moving => {
                    let (start, end) = (field.point(runway.start), field.point(runway.end));
                    if let Some(way) = Facing::along([end.x - start.x, end.z - start.z]) {
                        facing.set_if_neq(way);
                    }
                    Sortie::Pause {
                        until: now.after(spec.pause),
                    }
                }
                Sortie::Pause { until } if now >= until => {
                    dash(&mut facing, field.point(runway.end), step.get());
                    Sortie::Roll
                }
                Sortie::Roll if !moving => {
                    runways.release(place.runway, id);
                    goal.set_if_neq(FlightGoal::Height);
                    Sortie::FLYING
                }
                held => held,
            };
            sortie.set_if_neq(next);
            let grounded = status.turned(Airfields::GROUNDED, next.grounded());
            status.set_if_neq(grounded);
        }
    }

    /// Frees the runway each unit of a sortie that died held, as it dies, so the next unit of its
    /// airfield takes it (jets.md, A jet takes off and lands on its runway).
    pub(crate) fn free_runways(
        (index, airfields): (Res<'_, EntityIndex>, Res<'_, ById<UnitType, AirfieldData>>),
        dead: DeadSorties<'_, '_>,
        mut fields: Query<'_, '_, (&UnitType, &mut Runways)>,
    ) {
        for (&unit, &MadeAt { producer, place }) in &dead {
            let field = index
                .get(producer)
                .and_then(|producer| fields.get_mut(producer).ok());
            let Some((&field_type, mut runways)) = field else {
                continue;
            };
            let airfield = airfields
                .get(field_type)
                .expect("an airfield's type has its airfield");
            let runway = airfield.places[usize::from(place)].runway;
            if runways.holder(runway) == Some(unit) {
                runways.release(runway, unit);
            }
        }
    }

    /// The point a unit at `step` a tick, of `flight` at `hz` ticks a second, flies home to: past
    /// the runway's `end`, away from its `start`, as far as it flies while it sinks from its
    /// height to the ground at its climb, the ticks of that rounded up.
    fn approach(
        field: Field,
        start: FramePoint,
        end: FramePoint,
        flight: &FlightData,
        step: MoveStep,
        hz: i64,
    ) -> Vec3 {
        let (start, end) = (field.point(start), field.point(end));
        let climb = flight
            .climb()
            .checked_div_int(hz, Rounding::NearestEven)
            .expect("a climb over a rate fits");
        let ticks = flight
            .height()
            .checked_div(climb, Rounding::Ceiling)
            .expect("a height over a climb fits")
            .ceil();
        let along = start.ground_offset(end).normalized();
        let along = along.unwrap_or(Vec3::new(Num::ONE, Num::ZERO, Num::ZERO));
        end.ground() + along * (step.get() * Num::int(ticks))
    }

    /// The ticks of `reload` for the share of a clip of `shots` that `fired` empties, rounded up.
    fn share(reload: Ticks, fired: u16, shots: u16) -> Ticks {
        let ticks = reload.get() * u64::from(fired);
        Ticks::new(ticks.div_ceil(u64::from(shots)))
    }

    /// Refills each clip of `slots` that refills at home, of `weapons`, in `clips`, by the share of
    /// a reload from `from` to `until` that passed by `now`: the share of its shots back, rounded
    /// down, as Zero Hour fills a clip in proportion (`JetOrHeliReloadAmmoState::update`).
    fn reload(
        clips: &mut ClipShots,
        slots: &ActionSlots,
        weapons: &ById<ActionId, Weapon>,
        now: Tick,
        from: Tick,
        until: Tick,
    ) {
        let total = until
            .since(from)
            .expect("a reload ends after it starts")
            .get();
        let passed = now.since(from).map_or(0, Ticks::get).min(total);
        for slot in slots {
            let (Some(action), Some(rank)) = (slot.action, slot.rank) else {
                continue;
            };
            let clip = weapons
                .get(action)
                .and_then(|weapon| weapon.clip(&[], rank));
            let Some(clip) = clip.filter(|clip| clip.refill == ClipRefill::Home) else {
                continue;
            };
            let shots = NonZeroU64::from(clip.shots).get();
            let back =
                u16::try_from(shots * passed / total).expect("a share of a clip's shots fits");
            let fired = clips.fired(action).min(clip.shots.get() - back);
            if fired != clips.fired(action) {
                clips.refill(action, fired);
            }
        }
    }

    /// Sends the unit of `entity` at `from` straight to `to` by a dash of `step` a tick, which
    /// carries it as a forced move does, facing along its way.
    fn dash(
        commands: &mut Commands<'_, '_>,
        entity: Entity,
        facing: &mut Mut<'_, Facing>,
        from: Vec3,
        to: Vec3,
        step: Num,
    ) {
        if let Some(way) = Facing::along([to.x - from.x, to.z - from.z]) {
            facing.set_if_neq(way);
        }
        commands.entity(entity).insert(ForcedMove::Dash {
            to: DashTo::Point(to),
            step,
        });
    }
}
