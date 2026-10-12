#[cfg(test)]
pub(crate) mod test_world {
    use bevy_ecs::resource::Resource;
    use bevy_ecs::world::World;
    use campfire_math::{Num, Vec3};
    use campfire_sim::{EntityIndex, Position, StableId, TickRate};

    use crate::projectiles::launches::{Launch, LaunchPayload, Launches};
    use crate::projectiles::projectile::{Flight, Payload, Projectile};
    use crate::projectiles::projectile_data::ProjectileData;
    use crate::projectiles::projectile_spec::ProjectileSpec;
    use crate::units::action_id::ActionId;
    use crate::units::by_type::ByType;
    use crate::units::engine_tag::EngineTag;
    use crate::units::unit_type::UnitType;
    use crate::units::view::View;
    use crate::values::damage_kind::DamageKind;
    use crate::values::rank::Rank;

    /// What the projectiles module gives a world for its own tests.
    pub(crate) trait ProjectilesWorldTest {
        /// Makes `unit_type` a projectile type of `data`, tagged `projectile`, its speed a tick at
        /// the match's rate, which the package load checked: its `hits` filter names tags of the
        /// match's.
        fn load_projectile_type(&mut self, unit_type: UnitType, data: &ProjectileData);

        /// The projectile types of a volley's match.
        fn volley(&self) -> Volley;

        /// Launches a projectile of the line type `unit_type` from `source` at the origin along
        /// +x, carrying 10 damage, from the next tick.
        fn fire_line(&mut self, source: StableId, unit_type: UnitType);

        /// The roll each projectile carries, by stable id.
        fn rolls(&self) -> Vec<Num>;

        /// Where each projectile is, by stable id.
        fn projectiles(&self) -> Vec<Position>;
    }

    impl ProjectilesWorldTest for World {
        fn load_projectile_type(&mut self, unit_type: UnitType, data: &ProjectileData) {
            let rate = *self.resource::<TickRate>();
            let view = self.non_send::<View>().clone();
            let spec = {
                let mut types = view.types_mut();
                types.give_tag(unit_type, EngineTag::Projectile.tag());
                ProjectileSpec::of(data, &types, rate)
            };
            self.resource_mut::<ByType<ProjectileSpec>>()
                .set(unit_type, spec);
        }

        fn volley(&self) -> Volley {
            *self.resource::<Volley>()
        }

        fn fire_line(&mut self, source: StableId, unit_type: UnitType) {
            let range = self
                .resource::<ByType<ProjectileSpec>>()
                .get(unit_type)
                .unwrap()
                .range
                .unwrap();
            self.resource_mut::<Launches>().push(Launch {
                id: None,
                source,
                from: Position::ORIGIN,
                unit_type,
                flight: Flight::line(Vec3::new(Num::ONE, Num::ZERO, Num::ZERO), range, None),
                payload: LaunchPayload::Attack {
                    action: ActionId::new(0),
                    rank: Rank::FIRST,
                    amount: Num::int(10),
                    kind: DamageKind::new(0),
                    roll: Num::ZERO,
                },
            });
        }

        fn rolls(&self) -> Vec<Num> {
            let projectiles = self.resource::<EntityIndex>().iter();
            projectiles
                .filter_map(|(_, entity)| self.get::<Projectile>(entity))
                .map(|projectile| match projectile.payload() {
                    Payload::Attack { roll, .. } => roll,
                    Payload::Action { .. } => panic!("an attack's projectile"),
                })
                .collect()
        }

        fn projectiles(&self) -> Vec<Position> {
            self.resource::<EntityIndex>()
                .iter()
                .filter(|&(_, entity)| self.get::<Projectile>(entity).is_some())
                .map(|(_, entity)| *self.get::<Position>(entity).unwrap())
                .collect()
        }
    }

    /// The projectile types a volley's match loads.
    #[derive(Resource, Debug, Clone, Copy)]
    pub(crate) struct Volley {
        /// A homing projectile type.
        pub(crate) bolt: UnitType,
        /// A line of 1 m wide for 6 m, that passes through what it hits.
        pub(crate) lance: UnitType,
        /// A line of no width for 6 m, that ends at its first hit.
        pub(crate) dart: UnitType,
        /// A line for 6 m that hits nothing.
        pub(crate) scout: UnitType,
    }
}
