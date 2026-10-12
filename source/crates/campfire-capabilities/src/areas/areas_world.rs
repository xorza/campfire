#[cfg(test)]
pub(crate) mod test_world {
    use bevy_ecs::world::World;
    use campfire_sim::TickRate;

    use crate::areas::area_data::AreaData;
    use crate::areas::area_spec::AreaSpec;
    use crate::stats::Stats;
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
                Stats::modifier(self, package, name.as_str())
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
