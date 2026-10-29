use bevy_ecs::entity::Entity;
use campfire_geometry::{BodyBox, Shape};
use campfire_math::{Direction, Num, Vec3};
use campfire_sim::StableId;
use campfire_units::Layer;
use campfire_values::Team;

use crate::body_index::{BodyIndex, IndexedBody};
use crate::collider::contact::{Contact, ContactBodies};
use crate::moves::Moves;

pub(crate) mod contact;

/// A living unit's body as collision sees it: where it stands, its shape, its layer, whether it may
/// be pushed, whether it walks now, to a destination, and whether it gathers, as a worker in its
/// gather loop, which passes through another; its weight against another body it parts from, and,
/// for a unit that drives, the way it faces, the one way it may be pushed; its team; whether it is
/// firm, as one that attacks, works, uses an ability or that its tags root, which no push moves
/// (04-capabilities/navigation.md, Who may move whom is the game's); whether it leaves its
/// producer, passing through its team's units (04-capabilities/navigation.md, A new unit leaves its
/// exit); the unit its walk goes into, if any, which it passes through, either way
/// (entries-and-blocking.md, A walker passes through the unit it walks into); and whether a static
/// body parted it this tick, which pins it against the other walkers. A unit that cannot walk,
/// such as a tower or a building, is never pushed; only such a unit has a box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Collider {
    pub id: StableId,
    pub(crate) entity: Entity,
    pub(crate) point: Vec3,
    pub shape: Shape,
    pub(crate) layer: Layer,
    pub(crate) movable: bool,
    pub(crate) walking: bool,
    pub(crate) gathering: bool,
    pub(crate) weight: Num,
    pub(crate) way: Option<Direction<2>>,
    pub(crate) team: Option<Team>,
    pub(crate) firm: bool,
    pub(crate) leaving: bool,
    pub(crate) into: Option<StableId>,
    pub(crate) pinned: bool,
}

/// A pass of collision over a tick's contacts: one of the early ones, in which a unit that drives
/// moves only along its facing, or the last, in which it may be pushed off it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pass {
    Early,
    Last,
}

/// What bounds a tick's pushes: whom the mode lets a walker move, and the static bodies a push
/// may not end in.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Pushes<'a> {
    pub(crate) moves: Moves,
    pub(crate) statics: &'a BodyIndex,
}

/// How one body lies from another that it overlaps: the offset to it along x and z, and the two
/// radii together, in bits of a `Num`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Overlap {
    dx: i128,
    dz: i128,
    reach: i128,
}

impl Collider {
    /// The passes collision makes over a tick's contacts at most, each parting the pairs the
    /// passes before it left overlapping, as a crowd chains its contacts: 8, as two walkers
    /// wedged between two static bodies part across the wedge by a little each pass, and the
    /// proving match's trained units at its barracks need more than 4. The last lets a unit that
    /// drives be pushed off its facing, as two side by side part no other way
    /// (04-capabilities/navigation.md, Collision leaves no overlap).
    pub(crate) const PASSES: usize = 8;

    /// The share of a pair's reach, the two radii, it may still overlap by past the passes and
    /// count as parted: a 64th, as a chain of contacts shrinks its overlaps with each pass and
    /// never reaches none.
    const SLACK: i128 = 64;

    /// A body's personal space, as a share of its radius: 5/4, so two part before they touch,
    /// as 0 A.D. pushes within 8/5 of two clearances, here closer, as a body is a unit's whole
    /// footprint (04-capabilities/navigation.md, A unit that steps yields by a push).
    const SPACE: [i64; 2] = [5, 4];

    /// The share of its intrusion into the pair's personal space a push undoes in a tick: a
    /// quarter, so a pair parts over a few ticks, not in one jump.
    const PUSH: i128 = 4;

    /// Pushes apart each pair of walkers of `contacts` within their personal space of which one
    /// walks, in order, each pair seeing the pushes before it: each side the other may move,
    /// by `PUSH` of the intrusion, split by weight as an overlap is. A side that drives, is
    /// firm, or that the mode's `moves` lets the other not move takes none, and so does a side
    /// whose push would end in a static body. Two that both stand take none, so a crowd at rest
    /// settles.
    pub(crate) fn push(colliders: &mut [Self], contacts: &[Contact], pushes: Pushes<'_>) {
        let Pushes { moves, statics } = pushes;
        for contact in contacts {
            let ContactBodies::Walkers { first, second } = contact.bodies else {
                continue;
            };
            let (head, tail) = colliders.split_at_mut(second);
            let (one, two) = (&mut head[first], &mut tail[0]);
            let pushed = (one.yields_to(two, moves), two.yields_to(one, moves));
            if !(one.walking || two.walking) || pushed == (false, false) {
                continue;
            }
            let dx = i128::from(two.point.x.to_bits() - one.point.x.to_bits());
            let dz = i128::from(two.point.z.to_bits() - one.point.z.to_bits());
            let space = i128::from(one.space().to_bits() + two.space().to_bits());
            // Two on one spot have no way apart to push along; collision parts them.
            let Some(direction) = Direction::of([dx, dz]) else {
                continue;
            };
            if direction.length() >= space {
                continue;
            }
            let shove = direction.scaled_away((space - direction.length()) / Self::PUSH);
            let (own, others) = (one.weight.to_bits(), two.weight.to_bits());
            let back = shove.map(|along| match pushed {
                (true, true) => i128::from(others) * along / i128::from(own + others),
                (true, false) => along,
                _ => 0,
            });
            let ahead = [0, 1].map(|axis| shove[axis] - back[axis]);
            if pushed.0 {
                one.shift(back.map(|along| -along), statics);
            }
            if pushed.1 {
                two.shift(ahead, statics);
            }
        }
    }

    /// Whether `other`'s push may move it: it may be pushed, takes a push any way, is not firm,
    /// and `moves` lets `other` move it.
    fn yields_to(&self, other: &Self, moves: Moves) -> bool {
        self.movable
            && self.way.is_none()
            && !self.firm
            && self.may_separate(other)
            && !self.passes(other)
            && moves.may_move(other.team, self.team)
    }

    /// Moves it `by` bits along x and z, unless that ends in a body of `statics`.
    fn shift(&mut self, by: [i128; 2], statics: &BodyIndex) {
        let moved = Self {
            point: Self::moved(self.point, by),
            ..*self
        };
        if !moved.in_static(statics) {
            *self = moved;
        }
    }

    /// Whether it and `other` pass through each other: one leaves its producer, and the two are of
    /// its team.
    pub(crate) fn passes(&self, other: &Self) -> bool {
        (self.leaving || other.leaving) && self.team.is_some() && self.team == other.team
    }

    /// Whether it overlaps a body of `statics`.
    pub(crate) fn in_static(&self, statics: &BodyIndex) -> bool {
        statics.any_near(self.layer, self.point, self.radius(), |body| {
            self.overlaps(&Self::fixed(body))
        })
    }

    /// The radius of its personal space: its body's bound for one that may not be pushed, which
    /// takes no push, so a walker comes near it only where the walker's own space meets it.
    pub(crate) fn space(&self) -> Num {
        if !self.movable {
            return self.shape.bound();
        }
        Self::space_of(self.radius())
    }

    /// The radius of the personal space of a body that may be pushed, of `radius`.
    pub(crate) const fn space_of(radius: Num) -> Num {
        let [times, per] = Self::SPACE;
        Num::from_bits(radius.to_bits() * times / per)
    }

    /// Whether it and `other` may part and lie within each other's personal space: for a box, the
    /// other's space overlaps it.
    pub(crate) fn near(&self, other: &Self) -> bool {
        if !self.may_separate(other) {
            return false;
        }
        if let Some((body, at, mover)) = self.boxed(other) {
            return body.push_out(at, mover.point, mover.space()).is_some();
        }
        let dx = i128::from(other.point.x.to_bits() - self.point.x.to_bits());
        let dz = i128::from(other.point.z.to_bits() - self.point.z.to_bits());
        let space = i128::from(self.space().to_bits() + other.space().to_bits());
        dx * dx + dz * dz < space * space
    }

    /// Pushes the two colliders of each of `contacts` apart along the line between them, on the
    /// ground plane, until their centers lie the sum of their radii apart, each coordinate of that
    /// offset rounded away from zero past the direction's own rounding, so they end touching or
    /// apart, less than a bit past it on each axis: in order, each contact seeing the pushes of
    /// those before it, in passes until one parts no pair or `PASSES` are made, then those of a
    /// walker and a static body once more, so a static body, which never moves, has the last word
    /// and no later push leaves a walker in it; a walker a static body parted holds against a
    /// walker that would share the overlap for the rest of the tick, so a pair beside a wall parts
    /// at once, not by halves; the count of contacts that still overlap deeper than `SLACK` of their reach, as a
    /// chain of walkers shrinks its overlaps with each pass and never ends them. A unit that
    /// walks into one that stands takes the whole overlap, so no unit shoves another aside; two
    /// that both walk, or both stand, share it, each by the other's weight over the two's, the
    /// higher id taking the odd bits; one that may not be pushed leaves the whole overlap to the
    /// other; a unit that drives moves only along its facing, and the other takes the rest. Two
    /// on one spot part along x, the higher id towards +x.
    pub(crate) fn resolve(colliders: &mut [Self], contacts: &[Contact]) -> usize {
        debug_assert!(colliders.is_sorted_by_key(|collider| collider.id));
        for pass in 0..Self::PASSES {
            let pass = if pass + 1 == Self::PASSES {
                Pass::Last
            } else {
                Pass::Early
            };
            let mut parted = false;
            for contact in contacts {
                parted |= Self::part(colliders, contact, pass);
            }
            if !parted {
                break;
            }
        }
        let statics = |contact: &&Contact| matches!(contact.bodies, ContactBodies::Static { .. });
        for contact in contacts.iter().filter(statics) {
            Self::part(colliders, contact, Pass::Last);
        }
        (contacts.iter())
            .filter(|contact| Self::deep(colliders, contact))
            .count()
    }

    /// Whether the bodies of `contact` overlap by more than `SLACK` of their reach, and do not
    /// pass through each other.
    fn deep(colliders: &[Self], contact: &Contact) -> bool {
        if Self::passing(colliders, contact) {
            return false;
        }
        let (one, two) = match contact.bodies {
            ContactBodies::Walkers { first, second } => (colliders[first], colliders[second]),
            ContactBodies::Static { walker, body } => (colliders[walker], Self::fixed(&body)),
        };
        if !one.may_separate(&two) {
            return false;
        }
        if let Some((body, at, mover)) = one.boxed(&two) {
            let Some(moved) = body.push_out(at, mover.point, mover.radius()) else {
                return false;
            };
            let slack = i128::from(mover.radius().to_bits()) / Self::SLACK;
            let [dx, dz] = [moved.x - mover.point.x, moved.z - mover.point.z]
                .map(|along| i128::from(along.to_bits()));
            return dx * dx + dz * dz > slack * slack;
        }
        one.overlap(&two).is_some_and(|Overlap { dx, dz, reach }| {
            let close = reach - reach / Self::SLACK;
            dx * dx + dz * dz < close * close
        })
    }

    /// Parts the bodies of `contact`, unless they overlap no more or pass through each other;
    /// whether they did. A unit that drives may be pushed off its facing only on the last pass.
    fn part(colliders: &mut [Self], contact: &Contact, pass: Pass) -> bool {
        if !Self::overlapping(colliders, contact) || Self::passing(colliders, contact) {
            return false;
        }
        match contact.bodies {
            ContactBodies::Walkers { first, second } => {
                debug_assert!(first < second);
                let (head, tail) = colliders.split_at_mut(second);
                head[first].separate(&mut tail[0], pass);
            }
            ContactBodies::Static { walker, body } => {
                let (walker, mut fixed) = (&mut colliders[walker], Self::fixed(&body));
                if walker.id < body.id {
                    walker.separate(&mut fixed, pass);
                } else {
                    fixed.separate(walker, pass);
                }
                walker.pinned = true;
            }
        }
        true
    }

    /// Marks in `among`, by place, each of `colliders` that passes through a unit of its team it
    /// overlaps among `contacts`: one that leaves its producer and is not marked has cleared
    /// them.
    pub(crate) fn passing_through(colliders: &[Self], contacts: &[Contact], among: &mut Vec<bool>) {
        among.clear();
        among.resize(colliders.len(), false);
        for contact in contacts {
            if let ContactBodies::Walkers { first, second } = contact.bodies
                && Self::passing(colliders, contact)
                && Self::overlapping(colliders, contact)
            {
                among[first] = true;
                among[second] = true;
            }
        }
    }

    /// Whether the bodies of `contact` pass through each other, two walkers as `passes` says.
    fn passing(colliders: &[Self], contact: &Contact) -> bool {
        match contact.bodies {
            ContactBodies::Walkers { first, second } => colliders[first].passes(&colliders[second]),
            ContactBodies::Static { .. } => false,
        }
    }

    /// Whether the bodies of `contact` overlap.
    fn overlapping(colliders: &[Self], contact: &Contact) -> bool {
        match contact.bodies {
            ContactBodies::Walkers { first, second } => {
                colliders[first].overlaps(&colliders[second])
            }
            ContactBodies::Static { walker, body } => {
                colliders[walker].overlaps(&Self::fixed(&body))
            }
        }
    }

    /// The static body `body` as a collider: never pushed, never walking, and never gathering,
    /// as only a walker gathers.
    pub(crate) const fn fixed(body: &IndexedBody) -> Self {
        Self {
            id: body.id,
            entity: Entity::PLACEHOLDER,
            point: body.position,
            shape: body.shape,
            layer: body.layer,
            movable: false,
            walking: false,
            gathering: false,
            weight: Num::ONE,
            way: None,
            team: None,
            firm: false,
            leaving: false,
            into: None,
            pinned: false,
        }
    }

    /// Whether the bodies of `self` and `other` overlap on the ground plane, exactly: touching is
    /// not overlap. Two that may not be pushed never part, so they have no contact, nor have two
    /// of other layers, nor two that both gather.
    pub(crate) fn overlaps(&self, other: &Self) -> bool {
        if !self.may_separate(other) {
            return false;
        }
        match self.boxed(other) {
            Some((body, at, mover)) => body.push_out(at, mover.point, mover.radius()).is_some(),
            None => self.overlap(other).is_some(),
        }
    }

    /// Whether the two may separate: they stand on one layer, one of them may be pushed, not both
    /// gather, and neither walks into the other.
    fn may_separate(&self, other: &Self) -> bool {
        self.layer == other.layer
            && (self.movable || other.movable)
            && !(self.gathering && other.gathering)
            && self.into != Some(other.id)
            && other.into != Some(self.id)
    }

    /// The radius of a body that may be pushed, which is a circle.
    pub fn radius(&self) -> Num {
        match self.shape {
            Shape::Circle(radius) => radius.get(),
            Shape::Box(_) => panic!("a box is never pushed, so it has no radius to reach by"),
        }
    }

    /// The box of it and `other`, where it stands, and the other, which may be pushed: none
    /// when neither is a box. Only a body that may not be pushed is one.
    const fn boxed<'a>(&'a self, other: &'a Self) -> Option<(BodyBox, Vec3, &'a Self)> {
        let (body, at, mover) = match (self.shape, other.shape) {
            (Shape::Box(body), _) => (body, self.point, other),
            (_, Shape::Box(body)) => (body, other.point, self),
            _ => return None,
        };
        Some((body, at, mover))
    }

    /// How `other` lies from `self`, two circles, when the two overlap and may part, in bits of a
    /// `Num`: a position and a radius are within 2⁴⁵ bits, so squares fit i128.
    fn overlap(&self, other: &Self) -> Option<Overlap> {
        if !self.may_separate(other) {
            return None;
        }
        let dx = i128::from(other.point.x.to_bits() - self.point.x.to_bits());
        let dz = i128::from(other.point.z.to_bits() - self.point.z.to_bits());
        let reach = i128::from(self.shape.bound().to_bits() + other.shape.bound().to_bits());
        let square = dx * dx + dz * dz;
        (square < reach * reach).then_some(Overlap { dx, dz, reach })
    }

    fn separate(&mut self, other: &mut Self, pass: Pass) {
        if !self.may_separate(other) {
            return;
        }
        if let Some((body, at, _)) = self.boxed(other) {
            let mover = if other.movable { other } else { self };
            if let Some(moved) = body.push_out(at, mover.point, mover.radius()) {
                mover.point = moved;
            }
            return;
        }
        let Some(Overlap { dx, dz, reach }) = self.overlap(other) else {
            return;
        };
        let change = match Direction::of([dx, dz]) {
            None => [reach, 0],
            Some(direction) => {
                let parted = direction.scaled_away(reach);
                [0, 1].map(|axis| parted[axis] - [dx, dz][axis])
            }
        };
        let moves = match (self.movable, other.movable) {
            (true, true) if self.walking != other.walking => (self.walking, other.walking),
            (true, true) if self.pinned != other.pinned => (!self.pinned, !other.pinned),
            pair => pair,
        };
        let (own, others) = (self.weight.to_bits(), other.weight.to_bits());
        let mut back = change.map(|along| match moves {
            (true, true) => i128::from(others) * along / i128::from(own + others),
            (true, false) => along,
            _ => 0,
        });
        let mut ahead = [0, 1].map(|axis| change[axis] - back[axis]);
        let held = |way: Option<Direction<2>>| way.filter(|_| pass == Pass::Early);
        let ways = (held(self.way), held(other.way));
        if let Some(way) = ways.0 {
            back = Self::along(&way, back);
            if moves.1 && ways.1.is_none() {
                ahead = [0, 1].map(|axis| change[axis] - back[axis]);
            }
        }
        if let Some(way) = ways.1 {
            ahead = Self::along(&way, ahead);
            if moves.0 && ways.0.is_none() {
                back = [0, 1].map(|axis| change[axis] - ahead[axis]);
            }
        }
        self.point = Self::moved(self.point, back.map(|along| -along));
        other.point = Self::moved(other.point, ahead);
    }

    /// `push`, in bits along x and z, along `way` alone: its part along the way, rounded to
    /// nearest.
    fn along(way: &Direction<2>, push: [i128; 2]) -> [i128; 2] {
        let length = way.along(push);
        let scaled = way.scaled_away(length.abs());
        if length < 0 {
            scaled.map(|axis| -axis)
        } else {
            scaled
        }
    }

    /// `point` moved by `by` bits along x and z; its height stays.
    fn moved(point: Vec3, by: [i128; 2]) -> Vec3 {
        let bits = |along: i128| i64::try_from(along).expect("a push is shorter than two radii");
        Vec3::new(
            Num::from_bits(point.x.to_bits() + bits(by[0])),
            point.y,
            Num::from_bits(point.z.to_bits() + bits(by[1])),
        )
    }
}

#[cfg(test)]
mod tests;
