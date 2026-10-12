use bevy_ecs::world::{Mut, World};

use crate::players::players_world::PlayersWorld;
use crate::units::relations::Relations;

/// What the units module gives a world, for every crate.
pub trait UnitsWorld {
    /// The relations of the match, once its entity spawned, or reached a client.
    fn held_relations(&self) -> Option<&Relations>;
}

impl UnitsWorld for World {
    fn held_relations(&self) -> Option<&Relations> {
        self.get(self.match_entity()?)
    }
}

/// What the units module gives a world, for its own crate.
pub(crate) trait UnitsWorldCrate {
    /// The relations of the match, on its entity, which every match spawns as it
    /// starts.
    fn relations(&self) -> &Relations;

    /// The relations of the match, to change.
    fn relations_mut(&mut self) -> Mut<'_, Relations>;
}

impl UnitsWorldCrate for World {
    fn relations(&self) -> &Relations {
        self.held_relations()
            .expect("a match that started holds its relations")
    }

    fn relations_mut(&mut self) -> Mut<'_, Relations> {
        let entity = self
            .match_entity()
            .expect("a match that started has its entity");
        self.get_mut(entity)
            .expect("the match's entity holds the relations")
    }
}

#[cfg(test)]
pub(crate) mod test_world {
    use std::sync::Arc;

    use bevy_ecs::bundle::Bundle;
    use bevy_ecs::change_detection::Mut;
    use bevy_ecs::world::World;
    use campfire_math::Num;
    use campfire_script::{ScriptError, ScriptHost, ScriptId};
    use campfire_sim::{Position, StableId};

    use crate::combat::internals::Armed;
    use crate::combat::on_death::OnDeath;
    use crate::release::release_world::test_world::ReleaseWorldTest;
    use crate::scripts::script_book::ScriptBook;
    use crate::stats::stats_column::StatsColumn;
    use crate::units::Units;
    use crate::units::team::Team;
    use crate::units::type_scope::TypeScope;
    use crate::units::unit_type::UnitType;
    use crate::units::unit_type_data::UnitTypeData;
    use crate::units::units_column::UnitsColumn;
    use crate::units::view::View;
    use crate::values::declared_name::DeclaredName;
    use crate::values::scalar::Scalar;

    /// What the units module gives a world for its own tests.
    pub(crate) trait UnitsWorldTest {
        /// Compiles `source`, and adds the hooks the host reads of it to the match's book, as a
        /// test match has no package load to read them.
        fn compile_hooked(&mut self, source: &str) -> Result<ScriptId, ScriptError>;

        /// Names, for the scripts of a match with no mode, its `damage_kinds`, its `pools` and its
        /// players' `resources`, each by id, as a mode's books name them.
        fn name_kinds(&self, damage_kinds: &[&str], pools: &[&str], resources: &[&str]);

        /// Declares every tag the match's packages name, in their order, after the engine's tags,
        /// before any type, modifier or filter names one, so the tags are numbered the same however
        /// the packages load.
        fn declare_tags<'a>(&mut self, names: impl IntoIterator<Item = &'a str>);

        /// Loads the unit type `name` of `scope`, with its core fields: its tags, its params and
        /// its state fields, which the match's book of them and the view then hold. A name is one
        /// type's only in its scope.
        fn load_unit_type(&mut self, scope: TypeScope, name: &str, data: &UnitTypeData)
        -> UnitType;

        /// Loads `data` as the next unit type of the mode, named for its place.
        fn load_next_unit_type(&mut self, data: &UnitTypeData) -> UnitType;

        fn tagged_type(&mut self, tags: &[&str], params: &[(&str, Scalar)]) -> UnitType;

        /// A unit of 10 health, the life pool the first of the scene's pools, `health` and `mana`,
        /// with a weapon of 2 m, of the match's own type and `team`, with `parts`.
        fn armed_unit(&mut self, pos: Position, team: u8, parts: impl Bundle) -> StableId;

        /// An `armed_unit` of `unit_type`.
        fn armed_unit_of(
            &mut self,
            unit_type: UnitType,
            pos: Position,
            team: u8,
            parts: impl Bundle,
        ) -> StableId;
    }

    impl UnitsWorldTest for World {
        fn compile_hooked(&mut self, source: &str) -> Result<ScriptId, ScriptError> {
            let script = Units::compile(self, source)?;
            self.resource_scope(|world, mut scripts: Mut<'_, ScriptBook>| {
                assert_eq!(script.index(), scripts.len(), "the book holds every script");
                scripts.push(world.non_send::<ScriptHost>().functions(script));
            });
            Ok(script)
        }

        fn name_kinds(&self, damage_kinds: &[&str], pools: &[&str], resources: &[&str]) {
            let names = |names: &[&str]| -> Arc<[DeclaredName]> {
                names
                    .iter()
                    .map(|name| DeclaredName::new(name).expect("a test names a name"))
                    .collect()
            };
            let view = self.non_send::<View>();
            view.set_mode_names(&names(damage_kinds), names(resources));
            StatsColumn::share_pool_names(view, names(pools));
        }

        fn declare_tags<'a>(&mut self, names: impl IntoIterator<Item = &'a str>) {
            let view = self.non_send::<View>();
            let mut types = view.types_mut();
            for name in names {
                types.declare(name);
            }
        }

        fn load_unit_type(
            &mut self,
            scope: TypeScope,
            name: &str,
            data: &UnitTypeData,
        ) -> UnitType {
            let view = self.non_send::<View>().clone();
            // A package's own scope is that package's; the mode's, the mode package's.
            let package = match scope {
                TypeScope::Mode => 0,
                TypeScope::Package(package) => package,
            };
            let unit_type = view.types_mut().load(scope, package, name, data);
            view.share_type_names();
            let states = view.types_mut().state_book();
            UnitsColumn::share(&view, states.clone());
            self.insert_resource(states);
            unit_type
        }

        fn load_next_unit_type(&mut self, data: &UnitTypeData) -> UnitType {
            let name = format!("type {}", self.non_send::<View>().types_count());
            self.load_unit_type(TypeScope::Mode, &name, data)
        }

        fn tagged_type(&mut self, tags: &[&str], params: &[(&str, Scalar)]) -> UnitType {
            let data = UnitTypeData::of(tags, params);
            self.load_next_unit_type(&data)
        }

        fn armed_unit(&mut self, pos: Position, team: u8, parts: impl Bundle) -> StableId {
            let unit_type = self.test_unit_type();
            self.armed_unit_of(unit_type, pos, team, parts)
        }

        fn armed_unit_of(
            &mut self,
            unit_type: UnitType,
            pos: Position,
            team: u8,
            parts: impl Bundle,
        ) -> StableId {
            let armed = Armed::melee(Num::int(10), Num::int(2), 0, 1, Num::ZERO)
                .on_death(OnDeath::Stay)
                .bundle(self);
            self.spawn_unit_of(unit_type, pos, Team::new(team), (armed, parts))
        }
    }
}
