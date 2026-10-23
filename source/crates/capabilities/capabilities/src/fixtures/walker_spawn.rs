use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_navigation::Navigation;
use campfire_sim::StableId;
use campfire_sim::internals::SimWorldInternals;
use campfire_units::internals::Spawn;
use campfire_units::{BodyForm, BodyShape, Destination, Layer, MoveStep};
use campfire_values::Team;

/// A navigation test's unit of the mode's type `walker`, which `NavigationFixtures::walker_at`
/// starts: where it spawns, on which team and facing which angle, the body it has, if any, and
/// how it walks, if it does.
#[derive(Debug, Clone, Copy)]
pub struct WalkerSpawn {
    spawn: Spawn,
    angle: Num,
    form: Option<BodyForm>,
    walk: Option<Stride>,
}

/// How a walker walks: `step` a tick, to `to`, or to nowhere.
#[derive(Debug, Clone, Copy)]
struct Stride {
    step: Num,
    to: Option<Vec3>,
}

impl WalkerSpawn {
    pub(crate) const fn new(spawn: Spawn) -> Self {
        Self {
            spawn,
            angle: Num::ZERO,
            form: None,
            walk: None,
        }
    }

    /// On `team`.
    #[must_use]
    pub const fn team(mut self, team: Team) -> Self {
        self.spawn = self.spawn.team(team);
        self
    }

    /// Turned `angle` degrees, its body with it.
    #[must_use]
    pub const fn angle(mut self, angle: Num) -> Self {
        self.angle = angle;
        self
    }

    /// With a body of `form`.
    #[must_use]
    pub const fn body(mut self, form: BodyForm) -> Self {
        self.form = Some(form);
        self
    }

    /// With a circle's body of `radius`.
    #[must_use]
    pub fn circle(self, radius: impl Into<Num>) -> Self {
        self.body(BodyForm::new(
            BodyShape::circle(radius.into()).unwrap(),
            Layer::FIRST,
        ))
    }

    /// Walking `step` a tick to `to`, or standing where it is with none.
    #[must_use]
    pub const fn walks(mut self, step: Num, to: Option<Vec3>) -> Self {
        self.walk = Some(Stride { step, to });
        self
    }

    /// Spawns it in `world`.
    pub fn place(self, world: &mut World) -> StableId {
        let spawn = self.spawn.angle(self.angle);
        let body = self.form.map(|form| form.at(self.angle));
        let walker = self
            .walk
            .map(|walk| Navigation::walker(MoveStep::new(walk.step).unwrap()));
        // A walker spawns whole, its step with it, so its spawn tells it from a static body.
        let id = match (body, walker) {
            (Some(body), Some(walker)) => spawn.parts((body, walker)).place(world),
            (Some(body), None) => spawn.parts(body).place(world),
            (None, Some(walker)) => spawn.parts(walker).place(world),
            (None, None) => spawn.place(world),
        };
        if let Some(Stride { to, .. }) = self.walk {
            *world.part_mut::<Destination>(id) = Destination::to(to);
        }
        id
    }
}
