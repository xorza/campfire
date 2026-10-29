use std::num::NonZeroI64;

use bevy_ecs::change_detection::{DetectChangesMut, Mut};
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Added, Has, With, Without};
use bevy_ecs::system::{Commands, Local, Query, Res};
use bevy_ecs::world::World;
use campfire_actions::{ActionProgress, ActionSlots, ProgressIndexes};
use campfire_combat::{
    ClipShots, Damage, DamageCause, DamagePass, Heal, HealCause, HomeReload, PassEntry, Weapon,
};
use campfire_common::Ticks;
use campfire_declarations::ById;
use campfire_math::{Num, Rounding, Vec3};
use campfire_navigation::FlightData;
use campfire_production::{Doors, FramePoint, HeldPlaces, MadeAt, ProducerSpec, Rehome, Sold};
use campfire_sim::{EntityIndex, Keyed, MatchRate, Ordered, Position, SimTick, StableId};
use campfire_units::{
    DashTo, Dead, Destination, Facing, FlightGoal, ForcedMove, Life, MoveStep, StatusTags,
    Unfinished, UnitsWorld,
};
use campfire_values::{ActionId, Team, UnitType};

use crate::Airfields;
use crate::airfield_data::{AirfieldData, ApproachData};
use crate::runways::Runways;
use crate::sortie::Sortie;
use crate::sortie_data::SortieSpec;

/// The sorties of the units airfields park: each one's way from its place, along its runway, into
/// the air, and back.
#[derive(Debug)]
pub(crate) struct Sorties;

/// The parts of an airfield a sortie reads and writes: its id, place, facing, type, team, runways
/// and places; of one that stands built and unsold, as Zero Hour's `findSuitableAirfield` refuses
/// one under construction or sold.
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
        (
            &'static mut Runways,
            &'static mut HeldPlaces,
            Option<&'static mut Doors>,
        ),
    ),
    (
        Without<Dead>,
        Without<Sortie>,
        Without<Unfinished>,
        Without<Sold>,
    ),
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

/// The units of a sortie their airfields keep: each one's id, type, sortie, airfield and life.
type KeptUnits<'w, 's> = Query<
    'w,
    's,
    (
        &'static StableId,
        &'static UnitType,
        &'static Sortie,
        &'static MadeAt,
        &'static Life,
    ),
    Without<Dead>,
>;

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
    /// ground, grounded, until it is ordered away (04-capabilities/production.md, A jet holds a
    /// place); one made at its place's exit taxis from there to its place first, as Zero Hour's jet
    /// leaves its hangar (04-capabilities/production.md, A hangar is a jet's start). One made at a
    /// place of a producer of no runways flies from there, as nothing would take it off.
    pub(crate) fn park(world: &mut World, entity: Entity) {
        let Some(&MadeAt { producer, place }) = world.get::<MadeAt>(entity) else {
            return;
        };
        let Some(producer) = world.resource::<EntityIndex>().get(producer) else {
            return;
        };
        let field = world.entity(producer);
        if !field.contains::<Runways>() {
            return;
        }
        let field = Field {
            at: field.get::<Position>().expect("an airfield stands").get(),
            angle: field
                .get::<Facing>()
                .map_or(Num::ZERO, |facing| facing.degrees()),
        };
        let field_type = *world
            .get::<UnitType>(producer)
            .expect("an airfield has a type");
        let unit_type = *world.get::<UnitType>(entity).expect("a unit has a type");
        let parked = world
            .resource::<ById<UnitType, ProducerSpec>>()
            .get(field_type)
            .expect("an airfield produces")
            .places[usize::from(place)];
        let taxi = parked.exit.zip(
            world
                .resource::<ById<UnitType, SortieSpec>>()
                .get(unit_type)
                .map(|spec| spec.taxi_step),
        );
        let mut unit = world.entity_mut(entity);
        let Some(mut sortie) = unit.get_mut::<Sortie>() else {
            return;
        };
        *sortie = if taxi.is_some() {
            Sortie::BackToPlace
        } else {
            Sortie::Parked
        };
        if let Some((exit, step)) = taxi {
            let mut facing = unit.get_mut::<Facing>().expect("a unit of a sortie faces");
            let dash = Self::dash(&mut facing, field.point(exit), field.point(parked.at), step);
            unit.insert(dash);
        }
        if let Some(mut goal) = unit.get_mut::<FlightGoal>() {
            *goal = FlightGoal::Ground;
        }
        if let Some(mut status) = unit.get_mut::<StatusTags>() {
            *status = status.turned(Airfields::HELD, true);
        }
    }

    /// Steps each unit of a sortie, by stable id, before the units move
    /// (04-capabilities/airfields.md, A jet takes off and lands on its runway;
    /// 04-capabilities/airfields.md, A jet goes home; 04-capabilities/airfields.md, An order waits
    /// for the air; 04-capabilities/airfields.md, A jet takes off as a jet). One ordered away from
    /// its place waits for its runway, holds it, taxis to its prep point and its start at its taxi
    /// speed, waits its pause, rolls from a stop toward its airfield's approach past the runway's
    /// end, lifting toward the approach's height, lets the runway go once past its end, and climbs
    /// from the approach, its orders its own. In the air, one whose clips that refill at home are
    /// all empty, or that stood idle its idle time, drops its attack and flies home to the
    /// approach; an order on the way takes one that still holds a shot back to the air. At the
    /// approach it sinks toward the approach's height and circles until it holds the runway, glides
    /// to its end at its least speed, rolls to its start, lets it go, taxis back to its place and
    /// reloads its clips, all in the longest time a clip's reload takes for the share it lacks, its
    /// shots back as the time goes. One whose airfield is gone takes the nearest airfield of its
    /// team with a free place; with none, as it goes home it circles where it stands, stranded,
    /// until one takes it. It is held, its walk and its attacks blocked, from its runway's approach
    /// to the approach as it takes off.
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
            let aims = progress
                .as_ref()
                .is_some_and(|progress| progress.attack_target().is_some());
            let ordered = destination.get().is_some() || aims;
            let empty = {
                let mut at_home = home_clips().peekable();
                at_home.peek().is_some() && at_home.all(|home| home.fired >= home.clip.shots.get())
            };
            let idle_since = match *sortie {
                Sortie::Flying { .. } if ordered => None,
                Sortie::Flying { idle_since } => idle_since.or(Some(now)),
                _ => None,
            };
            let idled = spec
                .idle
                .zip(idle_since)
                .is_some_and(|(idle, since)| now >= since.after(idle));
            let mut drop_target = || {
                if let Some(progress) = progress.as_mut() {
                    progress.set_attack_target(None, &mut indexes.of(id, entity));
                }
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
                        .filter_map(|((&field, &place, _), (_, &of), (_, held, _))| {
                            let free = held.free().filter(|_| of == team)?;
                            Some((place.get().ground_offset(at).length(), field, free))
                        });
                if let Some((_, field, place)) = free.min() {
                    let entity = index.get(field).expect("a living airfield is indexed");
                    let (_, (&field_type, _), (_, mut held, doors)) =
                        fields.get_mut(entity).expect("an airfield");
                    let times = producers.get(field_type).and_then(|spec| spec.doors);
                    let mut doors = doors.zip(times);
                    Rehome {
                        unit: id,
                        made: &mut made,
                        producer: field,
                        place,
                        held: &mut held,
                        doors: doors.as_mut().map(|(doors, times)| (&mut **doors, *times)),
                        now,
                    }
                    .apply();
                    home = Some(entity);
                }
            }
            let Some(home) = home else {
                // With no airfield to go to, one that goes home circles where it stands, losing
                // life, as Zero Hour's jet circles its dead airfield
                // (`JetOrHeliCirclingDeadAirfieldState`); one of a living producer of no runways
                // flies on.
                let ground = at.ground();
                let going = destination.get();
                let mut strand = || {
                    drop_target();
                    Destination::route_to(&mut destination, Some(ground));
                    Sortie::Stranded { at: ground }
                };
                let next = match *sortie {
                    Sortie::Flying { .. } if gone && (empty || idled) => strand(),
                    Sortie::Home => strand(),
                    // An order takes one that still holds a shot back to the air; one whose clips
                    // are empty circles on where it stands.
                    Sortie::Stranded { at: circled } => {
                        let away = aims || going.is_some_and(|to| to != circled);
                        match (away, empty) {
                            (false, _) => *sortie,
                            (true, false) => Sortie::FLYING,
                            (true, true) => strand(),
                        }
                    }
                    Sortie::Flying { .. } => Sortie::Flying { idle_since },
                    held => held,
                };
                sortie.set_if_neq(next);
                let held = status.turned(Airfields::HELD, next.held());
                status.set_if_neq(held);
                continue;
            };
            let ((_, &field_at, field_facing), (&field_type, _), (mut runways, ..)) =
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
            let flight = flights.get(unit_type).expect("a sortie's type flies");
            let (start, end) = (field.point(runway.start), field.point(runway.end));
            let approach = Self::approach(start, end, airfield.approach);
            let climb = flight
                .climb()
                .checked_div_int(hz, Rounding::NearestEven)
                .expect("a climb over a rate fits");
            // A jet lands at its least speed (`JetTakeoffOrLandingState::onEnter`).
            let landing = match flight.wings() {
                Some(wings) => wings
                    .least_speed()
                    .checked_div_int(hz, Rounding::NearestEven)
                    .expect("a speed over a rate fits"),
                None => step.get(),
            };
            let climbing = |lift: Num| FlightGoal::Toward {
                height: airfield.approach.height,
                climb: climb * lift,
            };
            let mut dash = |facing: &mut Mut<'_, Facing>, to: Vec3, step: Num| {
                commands
                    .entity(entity)
                    .insert(Self::dash(facing, at, to, step));
            };
            let next = match *sortie {
                Sortie::Flying { .. } if !empty && !idled => Sortie::Flying { idle_since },
                // One empty or idle goes home, and so does one stranded that has a home again.
                Sortie::Flying { .. } | Sortie::Stranded { .. } => {
                    drop_target();
                    Destination::route_to(&mut destination, Some(approach));
                    Sortie::Home
                }
                // On its way home, an order takes one that still holds a shot back to the air, as
                // Zero Hour's jet takes a command on its return (`JetAIUpdate::aiDoCommand`); one
                // whose clips are empty heeds none and flies on to its approach, where it
                // arrives as its walk there ends.
                Sortie::Home => {
                    let away = aims || destination.get().is_some_and(|to| to != approach);
                    match destination.get() {
                        _ if away && !empty => Sortie::FLYING,
                        None => {
                            goal.set_if_neq(climbing(Num::ONE));
                            Sortie::Approach
                        }
                        Some(to) if to == approach => Sortie::Home,
                        Some(_) => {
                            drop_target();
                            Destination::route_to(&mut destination, Some(approach));
                            Sortie::Home
                        }
                    }
                }
                // From the approach it glides to the runway's end at its least speed, sinking to
                // reach the ground there, then rolls to the runway's start, as Zero Hour's landing
                // path runs.
                Sortie::Approach if runways.take(place.runway, id) => {
                    let along = at.ground_offset(end).length();
                    let sink = if along > Num::ZERO {
                        at.y.checked_mul_div(landing, along, Rounding::Ceiling)
                            .expect("a height over a distance fits")
                    } else {
                        at.y
                    };
                    goal.set_if_neq(FlightGoal::Toward {
                        height: Num::ZERO,
                        climb: sink.min(Vec3::BOUND),
                    });
                    dash(&mut facing, end, landing);
                    Sortie::Descent
                }
                Sortie::Descent if !moving => {
                    goal.set_if_neq(FlightGoal::Ground);
                    dash(&mut facing, start, landing);
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
                        .map(|home| home.reload_ticks())
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
                        let total = until.since(from).expect("a reload ends after it starts");
                        let passed = now.since(from).unwrap_or(Ticks::ZERO).min(total);
                        let reload = HomeReload { passed, total };
                        clips.reload_at_home(slots, weapons, reload);
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
                // It rolls from a stop, gaining its acceleration a tick to its speed, toward the
                // approach past the runway's end, lifting more as it nears the end, and lets the
                // runway go once past it, as Zero Hour's takeoff path runs.
                Sortie::Pause { until } if now >= until => {
                    let rolled = spec.acceleration.min(step.get());
                    goal.set_if_neq(climbing(Self::lift(at, start, end)));
                    dash(&mut facing, approach, rolled);
                    Sortie::Roll { step: rolled }
                }
                Sortie::Roll { step: rolled } => {
                    let past = start.ground_offset(end).dot(end.ground_offset(at)) >= Num::ZERO;
                    if past || !moving {
                        runways.release(place.runway, id);
                    }
                    if moving {
                        let rolled = (rolled + spec.acceleration).min(step.get());
                        goal.set_if_neq(climbing(Self::lift(at, start, end)));
                        dash(&mut facing, approach, rolled);
                        Sortie::Roll { step: rolled }
                    } else {
                        goal.set_if_neq(FlightGoal::Height);
                        Sortie::FLYING
                    }
                }
                held => held,
            };
            sortie.set_if_neq(next);
            let held = status.turned(Airfields::HELD, next.held());
            status.set_if_neq(held);
        }
    }

    /// Frees the runway each unit of a sortie that died held, as it dies, so the next unit of its
    /// airfield takes it (04-capabilities/airfields.md, A jet takes off and lands on its runway).
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

    /// Kills each unit an airfield that died held, as it dies, from its runway's approach as it
    /// lands to its runway's end as it takes off, with no killer, as Zero Hour's airfield kills
    /// the jets parked at it, landing or taking off (`ParkingPlaceBehavior::killAllParkedUnits`;
    /// 04-capabilities/airfields.md, A jet goes home).
    pub(crate) fn kill_held(
        dead: Query<'_, '_, &StableId, (Added<Dead>, With<Runways>)>,
        units: Query<'_, '_, (&StableId, &MadeAt, &Sortie), Without<Dead>>,
        mut commands: Commands<'_, '_>,
    ) {
        for &field in &dead {
            for (&unit, made, sortie) in &units {
                if made.producer == field && sortie.held() {
                    commands.queue(move |world: &mut World| world.drain_life(unit));
                }
            }
        }
    }

    /// Heals each unit that rests at its airfield below its whole life by its airfield's heal, and
    /// deals each stranded unit its sortie's stranded loss, a tick's share of each, through
    /// combat's pass, by the stable id of the unit each reaches (04-capabilities/airfields.md, A
    /// jet goes home).
    pub(crate) fn upkeep(
        (rate, index): (Res<'_, MatchRate>, Res<'_, EntityIndex>),
        airfields: Res<'_, ById<UnitType, AirfieldData>>,
        sorties: Res<'_, ById<UnitType, SortieSpec>>,
        units: KeptUnits<'_, '_>,
        fields: Query<'_, '_, &UnitType, (With<Runways>, Without<Dead>)>,
        (mut commands, mut kept): (Commands<'_, '_>, Local<'_, Vec<PassEntry>>),
    ) {
        let hz = NonZeroI64::from(rate.hz()).get();
        let tick_of = |per_second: Num| {
            per_second
                .checked_div_int(hz, Rounding::NearestEven)
                .expect("an amount over a rate fits")
        };
        kept.clear();
        kept.extend(units.iter().filter_map(
            |(&id, &unit_type, &sortie, made, life)| match sortie {
                Sortie::Stranded { .. } => {
                    let loss = sorties.get(unit_type)?.stranded?;
                    Some(PassEntry::Damage(Damage {
                        source: None,
                        target: id,
                        amount: tick_of(loss.share * life.max()),
                        kind: loss.kind,
                        cause: DamageCause::Upkeep,
                        action: None,
                        depth: 0,
                        hit: None,
                    }))
                }
                _ if sortie.rests() && life.current() < life.max() => {
                    let &field_type = fields.get(index.get(made.producer)?).ok()?;
                    let heal = airfields.get(field_type)?.heal?;
                    Some(PassEntry::Heal(Heal {
                        source: Some(made.producer),
                        target: id,
                        amount: tick_of(heal),
                        cause: HealCause::Upkeep,
                        action: None,
                        depth: 0,
                    }))
                }
                _ => None,
            },
        ));
        kept.sort_unstable_by_key(|&entry| entry.target());
        for &entry in &*kept {
            commands.queue(move |world: &mut World| DamagePass::queue(world, entry));
        }
    }

    /// The ground point of the approach `approach` of the runway from `start` to `end`: past the
    /// end by its share of the runway's length, where a jet climbs out to and lands from
    /// (04-capabilities/airfields.md, A jet takes off as a jet).
    fn approach(start: Vec3, end: Vec3, approach: ApproachData) -> Vec3 {
        let past = start
            .ground_offset(end)
            .checked_mul(approach.past)
            .expect("a runway past its end lies within the world");
        end.ground() + past
    }

    /// The share of its climb a jet at `at` lifts by as it rolls down the runway from `start` to
    /// `end`: `(1 − d / L)²`, `d` its distance to the end and `L` the runway's length, from none at
    /// the start to all at the end, as `JetTakeoffOrLandingState::update` sets its lift.
    fn lift(at: Vec3, start: Vec3, end: Vec3) -> Num {
        let length = start.ground_offset(end).length();
        if length == Num::ZERO {
            return Num::ONE;
        }
        let left = at
            .ground_offset(end)
            .length()
            .checked_div(length, Rounding::NearestEven)
            .expect("a distance over a runway's length fits");
        let share = (Num::ONE - left).clamp(Num::ZERO, Num::ONE);
        share * share
    }

    /// The dash of `step` a tick that sends a unit at `from` straight to `to`, which carries it as
    /// a forced move does, its `facing` turned along its way.
    fn dash(facing: &mut Mut<'_, Facing>, from: Vec3, to: Vec3, step: Num) -> ForcedMove {
        if let Some(way) = Facing::along([to.x - from.x, to.z - from.z]) {
            facing.set_if_neq(way);
        }
        ForcedMove::Dash {
            to: DashTo::Point(to),
            step,
        }
    }
}
