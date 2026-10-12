#[cfg(test)]
pub(crate) mod test_world {
    use std::collections::BTreeMap;

    use bevy_ecs::bundle::Bundle;
    use bevy_ecs::entity::Entity;
    use bevy_ecs::resource::Resource;
    use bevy_ecs::schedule::{IntoScheduleConfigs, Schedules};
    use bevy_ecs::system::{Query, Res};
    use bevy_ecs::world::World;
    use campfire_common::Tick;
    use campfire_math::Num;
    use campfire_script::rhai::Dynamic;
    use campfire_script::{Budget, ScriptHost};
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{
        IdAllocator, Position, SimSet, SimTick, SimUpdate, SimWorld, SnapshotError, StableId,
        StateCopy, StateRegistry,
    };

    use crate::players::players_world::PlayersWorld;
    use crate::scripts::ctx::Ctx;
    use crate::scripts::error::CallError;
    use crate::stats::life::Life;
    use crate::units::block::Block;
    use crate::units::tag_book::TagBook;
    use crate::units::tag_data::TagData;
    use crate::units::team::Team;
    use crate::units::unit_core::UnitCore;
    use crate::units::unit_tags::UnitTags;
    use crate::units::unit_type::UnitType;
    use crate::units::unit_type_data::UnitTypeData;
    use crate::units::units_world::test_world::UnitsWorldTest;
    use crate::units::view::View;
    use crate::values::declared_name::DeclaredName;

    /// The type of the units a test spawns with no type of its own, which sealing the match's unit
    /// types loads.
    #[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
    struct TestUnitType(UnitType);

    /// What a capability's tests do to a match's world: spawn units as every spawn path does, read
    /// and change their parts by stable id, run ticks, and restore into a fresh match.
    pub(crate) trait ReleaseWorldTest {
        /// A unit of `team` at `pos` with `parts`, of the match's own type, `unit_type`, spawned as
        /// `spawn_of` spawns one.
        fn spawn_unit(&mut self, pos: Position, team: Team, parts: impl Bundle) -> StableId;

        /// A unit of `unit_type` and `team` at `pos`, of the next stable id, spawned as a mode's unit
        /// and a delivery are: `parts`, then its core, with its type's tags and its script state from
        /// the match's books. The first spawn seals the match's unit types, as a match loads them all
        /// before any unit spawns.
        fn spawn_unit_of(
            &mut self,
            unit_type: UnitType,
            pos: Position,
            team: Team,
            parts: impl Bundle,
        ) -> StableId;

        /// The unit of `core`, with `parts`, spawned as `spawn_of` spawns one.
        fn spawn_core(&mut self, core: UnitCore, parts: impl Bundle) -> StableId;

        /// The type of the units a test spawns with no type of its own: one of no tag, param or
        /// state field, which sealing the match's unit types loads.
        fn test_unit_type(&mut self) -> UnitType;

        /// Seals the match's unit types, once, with no effect for any tag but an engine tag's own,
        /// as `seal_types_with` does.
        fn seal_types(&mut self);

        /// Seals the match's unit types: loads the match's own type, and gives the tag book their
        /// tags and `effects`, by tag, as a match's books take them once its packages load. A test
        /// that put a tag book of its own sealed them itself, and its match has no type of its own.
        fn seal_types_with(&mut self, effects: &BTreeMap<DeclaredName, TagData>);

        fn unit_entity(&self, id: StableId) -> Entity;

        fn insert_parts(&mut self, id: StableId, parts: impl Bundle);

        /// Restores its state into `fresh`, a match that loaded the same books, as a restore does; the
        /// error that refuses it, if one does. A match whose spawns sealed its unit types seals
        /// `fresh`'s, which loads the same type of its own.
        fn refusal_into(&self, fresh: &mut World) -> Option<SnapshotError>;

        /// Gives unit `id` tags of no name that block `blocks`, in place of its own.
        fn set_blocks(&mut self, id: StableId, blocks: &[Block]);

        /// The life unit `id` has left, exactly.
        fn life(&self, id: StableId) -> Num;

        /// The life unit `id` has left, a whole amount.
        fn health(&self, id: StableId) -> i64;

        /// Despawns the match's entity it started with, as a restore takes a world of no entity.
        fn clear(&mut self);

        /// Restores its state into `fresh`, a match that loaded the same books, as a restore does, and
        /// asserts that the two hash the same.
        fn restore_into(&self, fresh: &mut World);

        /// Gives unit `id` tags of no name that block `blocks` in `stage` of tick `tick`, as a modifier
        /// applied in that stage would.
        fn block_at(&mut self, id: StableId, tick: u64, stage: SimSet, blocks: &'static [Block]);

        /// Brings `copy`, a copy of the match's state, up to the match, and checks that every state
        /// type of it hashes as the match's own.
        fn check_copy(&mut self, copy: &mut StateCopy);

        /// `probe(ctx, of)` in `source`, run on the units as they are now.
        fn probe(&mut self, source: &str, of: StableId) -> Result<Dynamic, CallError>;

        /// `expression` evaluated with `ctx` and `of`, the unit `of`, on the units as they are now.
        fn read(&mut self, expression: &str, of: StableId) -> Result<Dynamic, CallError>;
    }

    impl ReleaseWorldTest for World {
        fn spawn_unit(&mut self, pos: Position, team: Team, parts: impl Bundle) -> StableId {
            let unit_type = self.test_unit_type();
            self.spawn_unit_of(unit_type, pos, team, parts)
        }

        fn spawn_unit_of(
            &mut self,
            unit_type: UnitType,
            pos: Position,
            team: Team,
            parts: impl Bundle,
        ) -> StableId {
            let id = self.resource_mut::<IdAllocator>().allocate();
            let core = UnitCore {
                id,
                pos,
                unit_type,
                team,
                owner: None,
            };
            self.spawn_core(core, parts)
        }

        fn spawn_core(&mut self, core: UnitCore, parts: impl Bundle) -> StableId {
            self.seal_types();
            core.spawn(self, parts);
            core.id
        }

        fn test_unit_type(&mut self) -> UnitType {
            self.seal_types();
            self.get_resource::<TestUnitType>()
                .expect("a test that puts its own tag book spawns units of its own types")
                .0
        }

        fn seal_types(&mut self) {
            if self.contains_resource::<TestUnitType>() || self.contains_resource::<TagBook>() {
                return;
            }
            self.seal_types_with(&BTreeMap::new());
        }

        fn seal_types_with(&mut self, effects: &BTreeMap<DeclaredName, TagData>) {
            assert!(
                !self.contains_resource::<TestUnitType>(),
                "a match seals its unit types once"
            );
            let unit_type = self.load_next_unit_type(&UnitTypeData::default());
            self.insert_resource(TestUnitType(unit_type));
            let book = self.non_send::<View>().types_mut().tag_book(effects);
            self.insert_resource(book);
        }

        fn unit_entity(&self, id: StableId) -> Entity {
            self.entity_of(id).expect("a unit of the match")
        }

        fn insert_parts(&mut self, id: StableId, parts: impl Bundle) {
            let entity = self.unit_entity(id);
            self.entity_mut(entity).insert(parts);
        }

        fn refusal_into(&self, fresh: &mut World) -> Option<SnapshotError> {
            if let Some(&own) = self.get_resource::<TestUnitType>() {
                fresh.seal_types();
                assert_eq!(
                    fresh.get_resource::<TestUnitType>(),
                    Some(&own),
                    "the fresh match loaded the same types"
                );
            }
            fresh.clear();
            let mut snapshot = Vec::new();
            self.snapshot(&mut snapshot);
            fresh.restore(&snapshot).err()
        }

        fn set_blocks(&mut self, id: StableId, blocks: &[Block]) {
            self.insert_parts(id, UnitTags::blocking(blocks));
        }

        fn life(&self, id: StableId) -> Num {
            self.part::<Life>(id).current()
        }

        fn health(&self, id: StableId) -> i64 {
            self.life(id).to_int().expect("a whole amount of life")
        }

        fn clear(&mut self) {
            if let Some(entity) = self.match_entity() {
                self.despawn(entity);
            }
        }

        fn restore_into(&self, fresh: &mut World) {
            assert_eq!(
                self.refusal_into(fresh),
                None,
                "a match's own snapshot restores"
            );
            assert_eq!(
                fresh.state_hash(),
                self.state_hash(),
                "the restored match hashes as the match"
            );
        }

        fn block_at(&mut self, id: StableId, tick: u64, stage: SimSet, blocks: &'static [Block]) {
            let entity = self.unit_entity(id);
            let block = move |now: Res<'_, SimTick>, mut tags: Query<'_, '_, &mut UnitTags>| {
                if now.start() == Tick::new(tick) {
                    *tags.get_mut(entity).expect("a unit with tags") = UnitTags::blocking(blocks);
                }
            };
            self.resource_mut::<Schedules>()
                .add_systems(SimUpdate, block.in_set(stage));
        }

        fn check_copy(&mut self, copy: &mut StateCopy) {
            copy.follow(self);
            let (mut copied, mut own) = (Vec::new(), Vec::new());
            let registry = self.resource::<StateRegistry>();
            registry.hash_by_type(copy.world(), &mut copied);
            registry.hash_by_type(self, &mut own);
            let differ: Vec<&str> = copied
                .iter()
                .zip(&own)
                .filter(|(copied, own)| copied.hash != own.hash)
                .map(|(copied, _)| copied.name)
                .collect();
            assert!(
                differ.is_empty(),
                "the copy differs before tick {}: {differ:?}",
                self.now().get()
            );
        }

        fn probe(&mut self, source: &str, of: StableId) -> Result<Dynamic, CallError> {
            let ctx = self.non_send::<Ctx>().clone();
            ctx.view().read(self);
            let unit = ctx.view().unit(of).expect("a unit of the match");
            let mut host = self.non_send_mut::<ScriptHost>();
            let script = host.compile(source).expect("a probe compiles");
            let mut budget = Budget::new(u64::MAX);
            host.call(&mut budget, script, "probe", (ctx, unit))
                .map_err(CallError::from_script)
        }

        fn read(&mut self, expression: &str, of: StableId) -> Result<Dynamic, CallError> {
            self.probe(&format!("fn probe(ctx, of) {{ {expression} }}"), of)
        }
    }
}
