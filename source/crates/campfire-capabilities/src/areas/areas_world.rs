#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use bevy_ecs::world::World;
    use campfire_common::{Tick, Ticks};
    use campfire_math::Num;
    use campfire_sim::IdAllocator;

    use crate::areas::area::Area;
    use crate::areas::area_spec::{AreaSpec, Inside};
    use crate::deliveries::delivering::Delivering;
    use crate::units::action_id::ActionId;
    use crate::units::by_type::ByType;
    use crate::units::filter::Filter;
    use crate::units::unit_type::UnitType;
    use crate::values::rank::Rank;
    use crate::values::relation_set::RelationSet;

    /// What the areas module gives a world for the tests and benches of every crate.
    pub trait AreasWorldInternals {
        /// An area unit of `unit_type`, a type that reaches `radius`, as a client draws it: the
        /// type's spec in the match's book of them, and the area, from tick 0 to tick 1.
        fn standing_area(&mut self, unit_type: UnitType, radius: Num) -> Area;
    }

    impl AreasWorldInternals for World {
        fn standing_area(&mut self, unit_type: UnitType, radius: Num) -> Area {
            let spec = AreaSpec {
                radius,
                delay: Ticks::new(0),
                duration: Ticks::new(1),
                affects: Filter::of_relations(RelationSet::All),
                inside: Inside::default(),
            };
            self.get_resource_or_init::<ByType<AreaSpec>>()
                .set(unit_type, spec);
            let by = Delivering {
                source: IdAllocator::default().allocate(),
                action: ActionId::new(0),
                rank: Rank::FIRST,
                start: None,
                launch: None,
            };
            Area::new(by, None, Tick::new(1))
        }
    }
}

#[cfg(test)]
pub(crate) mod test_world {
    use bevy_ecs::world::World;
    use campfire_sim::TickRate;

    use crate::areas::area_data::AreaData;
    use crate::areas::area_spec::AreaSpec;
    use crate::stats::stats_world::internals::StatsWorldInternals;
    use crate::units::by_type::ByType;
    use crate::units::engine_tag::EngineTag;
    use crate::units::unit_type::UnitType;
    use crate::units::view::View;
    use crate::values::declared_name::DeclaredName;

    /// What the areas module gives a world for its own tests.
    pub(crate) trait AreasWorldTest {
        /// Makes `unit_type` of `package` an area type of `data`, tagged `area`, its times in
        /// ticks at the match's rate, rounded up, which the package load checked: its `affects`
        /// filter names tags of the match's, and its `inside` modifiers of the package.
        fn load_area_type(&mut self, unit_type: UnitType, package: u16, data: &AreaData);
    }

    impl AreasWorldTest for World {
        fn load_area_type(&mut self, unit_type: UnitType, package: u16, data: &AreaData) {
            let rate = *self.resource::<TickRate>();
            let view = self.non_send::<View>().clone();
            let modifier = |name: &DeclaredName| {
                self.modifier_named(package, name.as_str())
                    .expect("the load checked an area's modifiers")
            };
            let spec = {
                let mut types = view.types_mut();
                types.give_tag(unit_type, EngineTag::Area.tag());
                AreaSpec::of(data, &types, rate, modifier).expect("an area's time in ticks fits")
            };
            self.resource_mut::<ByType<AreaSpec>>().set(unit_type, spec);
        }
    }
}
