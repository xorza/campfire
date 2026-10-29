use bevy::ecs::change_detection::{DetectChanges, Ref};
use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::ecs::lifecycle::Remove;
use bevy::ecs::observer::On;
use bevy::ecs::query::{Added, Allow, Has};
use bevy::ecs::system::{Commands, Query, Res};
use bevy::math::Vec3;
use bevy::transform::components::Transform;
use campfire_sim::Unpredicted;
use campfire_units::Dead;

use crate::own_player::OwnPlayer;
use crate::view::drawing::{Drawing, DrawingOf, TornOff};
use crate::view::fog::MatchFog;

/// On the drawing of a unit that leaves the client alive, out of its player's team's sight: the
/// cell it stands in, where the client draws it as the team last saw it until the team sees the
/// cell again, as Zero Hour's `GhostObject` holds an immobile object in the fog (02-engine-core.md,
/// An immobile unit leaves a ghost). It draws no unit, so nothing picks, selects or gauges it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ghost {
    cell: usize,
}

/// A drawing whose unit left the client: the fog it leaves a ghost in, for one that leaves one,
/// whether it was torn off its unit, and where it stands.
#[derive(Debug, Clone, Copy)]
struct Left<'f> {
    leaves: Option<&'f MatchFog>,
    torn: bool,
    at: Vec3,
}

/// What stays of a drawing whose unit left: a torn one, which outlives its unit, a ghost, or
/// nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stays {
    Torn,
    Ghost(Ghost),
    Gone,
}

impl Left<'_> {
    /// What stays of it: a ghost of its cell, of the grid of the fog it leaves one in, which
    /// `clear` takes down where its team sees the cell.
    fn stays(self) -> Stays {
        if self.torn {
            return Stays::Torn;
        }
        match self.leaves.and_then(|fog| fog.cell_at(self.at)) {
            Some(cell) => Stays::Ghost(Ghost { cell }),
            None => Stays::Gone,
        }
    }
}

/// On the drawing of a unit that does not walk and does not die: what it leaves when it leaves.
#[derive(Component, Debug)]
pub(crate) struct LeavesGhost;

impl Ghost {
    /// As a drawing's unit leaves the client: keeps a drawing that leaves a ghost, in a match of
    /// fog, as a ghost of its cell, and takes every other down, but one torn off its unit, which
    /// outlives it. Whether the team sees the cell waits for `clear`, as the unit may leave
    /// before the cells the same message brings.
    pub(crate) fn leave(
        left: On<'_, '_, Remove<DrawingOf>>,
        roots: Query<'_, '_, (&Transform, Has<LeavesGhost>, Has<TornOff>)>,
        fog: Option<Res<'_, MatchFog>>,
        mut commands: Commands<'_, '_>,
    ) {
        let root = left.entity;
        let Ok((transform, leaves, torn)) = roots.get(root) else {
            return;
        };
        let drawing = Left {
            leaves: fog.as_deref().filter(|_| leaves),
            torn,
            at: transform.translation,
        };
        match drawing.stays() {
            Stays::Torn => {}
            Stays::Ghost(ghost) => {
                commands.entity(root).try_insert(ghost);
            }
            Stays::Gone => {
                commands.entity(root).try_despawn();
            }
        }
    }

    /// Takes the ghost of each dying unit away before it leaves: a death the client saw leaves
    /// none.
    pub(crate) fn forget_dying(
        dying: Query<'_, '_, &Drawing, (Added<Dead>, Allow<Unpredicted>)>,
        mut commands: Commands<'_, '_>,
    ) {
        for drawing in &dying {
            commands.entity(drawing.root()).try_remove::<LeavesGhost>();
        }
    }

    /// Takes each ghost down whose cell its player's team sees: each new one, and each as the
    /// cells change, once the frame's replication brought both.
    pub(crate) fn clear(
        own: OwnPlayer<'_, '_>,
        ghosts: Query<'_, '_, (Entity, Ref<'_, Self>)>,
        mut commands: Commands<'_, '_>,
    ) {
        // A spectator holds every unit, so a unit that leaves it is gone, and leaves no ghost.
        if own.player_slot().is_none() {
            for (root, _) in &ghosts {
                commands.entity(root).despawn();
            }
            return;
        }
        let Some(seen) = own.seen_cells() else {
            return;
        };
        let changed = seen.is_changed();
        for (root, ghost) in &ghosts {
            if (changed || ghost.is_added()) && seen.cells().holds(ghost.cell) {
                commands.entity(root).despawn();
            }
        }
    }
}

#[cfg(test)]
mod tests;
