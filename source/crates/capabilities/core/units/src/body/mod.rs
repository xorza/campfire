use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_geometry::{BodyBox, MAX_BOUND, Metric, Placed, Radius, Shape, Volume};
use campfire_math::{Num, Vec3};
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentOnce};
use campfire_visibility::DataKind;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};

use crate::layer::Layer;
use crate::map_book::MapBook;
use crate::unit_entity::UnitEntity;

/// A unit's body on the ground plane, on its layer: a circle of a radius, or a box, which a unit
/// that does not walk on a planar map may have, and its height, which makes it a volume
/// (04-capabilities/00-overview.md, A body has a height). Living bodies of one layer do not
/// overlap, and every range counts from a body's edge; a unit with no body is a point, on the first
/// layer.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[component(immutable)]
pub struct Body {
    shape: Shape,
    layer: Layer,
    height: Num,
}

impl Body {
    /// A circle on the first layer; `None` unless `radius` is positive and at most
    /// `MAX_BOUND`.
    pub const fn new(radius: Num) -> Option<Self> {
        if radius.to_bits() <= 0 {
            return None;
        }
        let Some(radius) = Radius::new(radius) else {
            return None;
        };
        Some(Self {
            shape: Shape::Circle(radius),
            layer: Layer::FIRST,
            height: Num::ZERO,
        })
    }

    /// The body of the built box `body`, on the first layer.
    pub const fn of_box(body: BodyBox) -> Self {
        Self {
            shape: Shape::Box(body),
            layer: Layer::FIRST,
            height: Num::ZERO,
        }
    }

    /// The body on `layer`.
    #[must_use]
    pub(crate) const fn on(self, layer: Layer) -> Self {
        Self { layer, ..self }
    }

    /// The body `height` meters tall; `None` unless `height` is at least 0 and at most
    /// `MAX_BOUND`, as every body's bound is.
    pub const fn raised(self, height: Num) -> Option<Self> {
        if !Self::holds_height(height) {
            return None;
        }
        Some(Self { height, ..self })
    }

    /// Whether a body may be `height` meters tall, as `raised` takes it.
    pub(crate) const fn holds_height(height: Num) -> bool {
        height.to_bits() >= 0 && height.to_bits() <= MAX_BOUND.to_bits()
    }

    pub const fn height(self) -> Num {
        self.height
    }

    pub const fn shape(self) -> Shape {
        self.shape
    }

    /// A circle's radius; `None` for a box.
    pub const fn radius(self) -> Option<Num> {
        match self.shape {
            Shape::Circle(radius) => Some(radius.get()),
            Shape::Box(_) => None,
        }
    }

    /// A box's half edges, `[x, z]` each; `None` for a circle.
    pub const fn half_edges(self) -> Option<[[Num; 2]; 2]> {
        match self.shape {
            Shape::Circle(_) => None,
            Shape::Box(body) => Some(body.half_edges()),
        }
    }

    pub const fn layer(self) -> Layer {
        self.layer
    }

    /// The shape of a unit with `body`, or a point for a unit with none.
    pub const fn shape_of(body: Option<&Self>) -> Shape {
        match body {
            Some(body) => body.shape,
            None => Shape::POINT,
        }
    }

    /// The volume of a unit with `body` at `position`, on the map of `map`: its shape from the
    /// position's point in space up by its height; a unit with no body is a point there.
    pub fn volume(body: Option<&Self>, position: Vec3, map: &MapBook) -> Volume {
        let base = Placed::new(map.point(position), Self::shape_of(body));
        Volume::new(base, body.map_or(Num::ZERO, |body| body.height))
    }

    /// The center of a unit with `body` at `position`, on the map of `map`: the position's point in
    /// space raised by half its height, which a shot aims at.
    pub fn center(body: Option<&Self>, position: Vec3, map: &MapBook) -> Vec3 {
        Self::volume(body, position, map).center()
    }

    /// The layer of a unit with `body`, or the first for a unit with none.
    pub const fn layer_of(body: Option<&Self>) -> Layer {
        match body {
            Some(body) => body.layer,
            None => Layer::FIRST,
        }
    }
}

impl SimComponent for Body {
    const NAME: &'static str = "units.body";
    type Host = UnitEntity;

    // Its decode bounds its radius or its box; a layer only compares with another. A box only on
    // a planar map, as the load gives one, which a spatial map's measures do not take.
    fn check(&self, world: &World, _: Entity) -> bool {
        matches!(self.shape, Shape::Circle(_))
            || world.resource::<MapBook>().metric() == Metric::Planar
    }
}

impl Replication for Body {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnce;
}

/// A snapshot is untrusted, so a radius `new` refuses fails to decode, a box its own decode
/// refuses, and a height `raised` refuses.
impl<'de> Deserialize<'de> for Body {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        struct Fields {
            shape: Shape,
            layer: Layer,
            height: Num,
        }
        let fields = Fields::deserialize(deserializer)?;
        let body = match fields.shape {
            Shape::Circle(radius) => Self::new(radius.get())
                .ok_or_else(|| D::Error::custom("a body radius not positive or beyond 2,048 m"))?,
            Shape::Box(body) => Self::of_box(body),
        };
        body.on(fields.layer)
            .raised(fields.height)
            .ok_or_else(|| D::Error::custom("a body height below 0 or beyond 2,048 m"))
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use campfire_geometry::{Metric, Placed, Radius, Shape};
    use campfire_math::{Num, Vec3};

    use crate::body::Body;

    impl Body {
        /// Whether `range` from the edge of `from_body` at `from` reaches the edge of `to_body`
        /// at `to`, on a planar map, by the reach rule; a unit with no body is a point.
        pub fn reaches(
            from: Vec3,
            from_body: Option<&Self>,
            range: Num,
            to: Vec3,
            to_body: Option<&Self>,
        ) -> bool {
            let from = Placed::new(from, Self::shape_of(from_body));
            Metric::Planar.reaches(from, range, Placed::new(to, Self::shape_of(to_body)))
        }

        /// Whether `range` from the edge of `from_body` at `from` reaches the least circle round
        /// `to` that holds `to_body`, as a body measured by its bound would be reached.
        pub fn reaches_bound(
            from: Vec3,
            from_body: Option<&Self>,
            range: Num,
            to: Vec3,
            to_body: Option<&Self>,
        ) -> bool {
            let bound = Radius::new(Self::shape_of(to_body).bound()).expect("a bound is a radius");
            let bound = Placed::new(to, Shape::Circle(bound));
            Metric::Planar.reaches(Placed::new(from, Self::shape_of(from_body)), range, bound)
        }

        /// Whether this circle at `position` overlaps the box `body` at `center`: a body collision
        /// left inside the box.
        pub fn sinks_into(&self, position: Vec3, body: &Self, center: Vec3) -> bool {
            let (Shape::Circle(radius), Shape::Box(boxed)) = (self.shape(), body.shape()) else {
                panic!("a circle and a box");
            };
            boxed.closer(center, position, radius.get())
        }
    }
}

#[cfg(test)]
mod tests;
