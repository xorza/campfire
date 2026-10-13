use bevy_ecs::world::World;
use campfire_actions::CapabilityDoesSpec;
use campfire_core::{CallError, Effect, Frame, MapBook, UnitType, View};
use campfire_deliveries::Delivering;
use campfire_sim::{Position, StableId};

use crate::Areas;

/// An area a call queued: of `by`, of `unit_type`, at `at`; with `id`, the id the call took for
/// it, which a script reads, or none for a launch of a list, which takes its id as it lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AreasEffect {
    pub id: Option<StableId>,
    pub(crate) by: Delivering,
    pub(crate) unit_type: UnitType,
    pub pos: Position,
}

impl AreasEffect {
    /// Queues the listed `does`, a launch, in `frame`: an area of its type, of the call's action
    /// at its rank from its acting unit, which runs the launch's lists, where `unit` stands as
    /// `view` reads it; none when `unit` is gone.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "its signature is the one every capability's listed effects queue by"
    )]
    pub(crate) fn queue_listed(
        does: CapabilityDoesSpec,
        id: StableId,
        _: Option<StableId>,
        frame: &mut Frame,
        view: &View,
    ) -> Result<(), CallError> {
        let CapabilityDoesSpec::Launch { area, launch } = does else {
            unreachable!("areas queue only their own listed effects")
        };
        let (Some(source), Some(action)) = (frame.acting(), frame.action()) else {
            unreachable!("a list runs for its action's acting unit")
        };
        // A unit gone, as a source despawned while its delivery flew, has no place: its area
        // lands nothing, as one whose source is gone does.
        let Some(row) = view.row(id) else {
            return Ok(());
        };
        let pos = row.pos;
        let by = Delivering {
            source,
            action,
            rank: frame.rank(),
            start: frame.start(),
            launch: Some(launch),
        };
        frame.effects.push(AreasEffect {
            id: None,
            by,
            unit_type: area,
            pos,
        });
        Ok(())
    }
}

impl Effect for AreasEffect {
    /// Applies the effect: an area that lands this tick, at the point of the map's bounds nearest
    /// where it says.
    fn apply(self, world: &mut World, _: &mut Frame) {
        let pos = world.resource::<MapBook>().bounds.clamp(self.pos);
        Areas::push(world, self.by, self.unit_type, pos, None, self.id);
    }
}
