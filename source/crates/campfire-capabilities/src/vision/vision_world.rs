#[cfg(test)]
pub(crate) mod test_world {
    use bevy_ecs::world::World;
    use campfire_math::{Num, Vec3};
    use campfire_script::{Budget, ScriptHost};
    use campfire_sim::internals::SimWorldInternals;
    use campfire_sim::{Position, SimTick, StableId};

    use crate::geometry::bounds::Bounds;
    use crate::geometry::grid::Grid;
    use crate::release::release_world::test_world::ReleaseWorldTest;
    use crate::scripts::ctx::Ctx;
    use crate::scripts::error::CallError;
    use crate::scripts::error::internals::FailureKind;
    use crate::stats::life::Life;
    use crate::units::tag_properties::TagProperties;
    use crate::units::team::Team;
    use crate::units::team_set::TeamSet;
    use crate::units::unit_tags::UnitTags;
    use crate::units::units_world::UnitsWorldCrate;
    use crate::vision::brush_map::BrushMap;
    use crate::vision::seen_by::SeenBy;
    use crate::vision::sight::Sight;
    use crate::vision::vision_column::VisionColumn;
    use crate::vision::vision_grid::VisionGrid;

    /// What the vision module gives a world for its own tests.
    pub(crate) trait VisionWorldTest {
        /// Gives the match a grid of one cell, for `teams` teams, that no Vision stage reads: the
        /// `SeenBy` a test sets on its units alone tells what each team sees.
        fn fog_grid(&mut self, teams: usize);

        /// A unit of `team` at `x`, `z` with the life pool, so it may be a target, and seeing
        /// `sight` when given.
        fn spawn_seer(&mut self, team: u8, x: i64, z: i64, sight: Option<i64>) -> StableId;

        /// Gives unit `id` tags with `properties`, as its modifiers would.
        fn set_properties(&mut self, id: StableId, properties: TagProperties);

        /// Runs `ctx.reveal(at.pos, radius, ms)` as `actor` thinks, or as the mode with no actor,
        /// then applies what it queued; why it failed, when it did.
        fn reveal(
            &mut self,
            actor: Option<StableId>,
            at: StableId,
            radius: &str,
            ms: i64,
        ) -> Result<(), FailureKind>;

        fn seen_by(&self, id: StableId) -> TeamSet;
    }

    impl VisionWorldTest for World {
        fn fog_grid(&mut self, teams: usize) {
            let bounds = Bounds::new([-Num::HALF; 2], [Num::HALF; 2]).unwrap();
            let grid = Grid::new(Num::ONE, bounds).unwrap();
            let brush = BrushMap::new(&grid, &[]);
            self.insert_resource(VisionGrid { grid, brush, teams });
        }

        fn spawn_seer(&mut self, team: u8, x: i64, z: i64, sight: Option<i64>) -> StableId {
            let life = Life::full(Num::int(100));
            let id = self.spawn_unit(
                Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::int(z))).unwrap(),
                Team::new(team),
                life,
            );
            if let Some(range) = sight {
                self.insert_parts(id, Sight::new(Num::int(range)).unwrap());
            }
            id
        }

        fn set_properties(&mut self, id: StableId, properties: TagProperties) {
            self.insert_parts(id, UnitTags::with_properties(properties));
        }

        fn reveal(
            &mut self,
            actor: Option<StableId>,
            at: StableId,
            radius: &str,
            ms: i64,
        ) -> Result<(), FailureKind> {
            let ctx = self.non_send::<Ctx>().clone();
            ctx.view().read(self);
            let now = self.resource::<SimTick>().start();
            match actor {
                Some(actor) => ctx.frame().begin_think(self, actor, now),
                None => ctx.frame().begin_mode(self, false, now),
            }
            let at = ctx.view().unit(at).unwrap();
            let source = format!("fn probe(ctx, at) {{ ctx.reveal(at.pos, {radius}, {ms}) }}");
            let returned = {
                let mut host = self.non_send_mut::<ScriptHost>();
                let script = host.compile(&source).unwrap();
                let mut budget = Budget::new(u64::MAX);
                host.call(&mut budget, script, "probe", (ctx.clone(), at))
            };
            let returned = returned.map_err(|error| CallError::from_script(error).kind())?;
            assert!(returned.is_unit());
            ctx.apply(self);
            Ok(())
        }

        fn seen_by(&self, id: StableId) -> TeamSet {
            let relations = self.relations();
            let parts = (self.try_part::<SeenBy>(id), self.try_part::<Team>(id));
            VisionColumn::seen_by(parts, relations)
        }
    }
}
