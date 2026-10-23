use std::ops::ControlFlow;

use campfire_geometry::{BodyGrid, Indexed, Placed};
use campfire_math::{Num, Vec3};
use campfire_sim::StableId;
use campfire_units::Layer;
use campfire_values::Team;

use crate::body_index::{BodyIndex, IndexedBody};
use crate::pathing_grid::PathingGrid;
use crate::route_planner::{RoutePlanner, Short, Walkable, Window};
use crate::segment::Segment;
use crate::walker::Walker;

/// Steers walkers round the units in their way, by short routes in a window of cells around each.
/// It keeps between ticks the index of the units that stand, made on the first tick with the
/// static index's buckets, the grid of those that walk, and the buffers each tick refills, so a
/// tick allocates nothing once they have grown.
#[derive(Debug, Default)]
pub(crate) struct Steering {
    standing: Option<BodyIndex>,
    /// The bodies of the units that stand, by stable id.
    still: Vec<IndexedBody>,
    /// The units in a gather loop, by stable id, which a walker in one passes through.
    gatherers: Vec<StableId>,
    /// The units that stand idle, by stable id, with their teams: those a stuck walker of their
    /// team moves aside.
    idle: Vec<(StableId, Team)>,
    /// The index of the units that stand and do not gather, for a walker that gathers; none
    /// until a unit gathers.
    apart: Option<BodyIndex>,
    still_apart: Vec<IndexedBody>,
    /// The bodies of the units that walk, by layer, which a stuck walker searches for those it
    /// touches, and the buffer they are read into.
    walking: BodyGrid<Layer, StableId>,
    moving: Vec<Indexed<Layer, StableId>>,
    /// The units that block the walker that steers now.
    blockers: Vec<IndexedBody>,
    /// The short route of the walker that steers now.
    short: Vec<Vec3>,
}

/// A walker that may steer this tick: its unit, where it stands, its team, if any, its step, its
/// kind, whether walkers keep it back, and whether it gathers, which passes through the others
/// that do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Steered {
    pub id: StableId,
    pub position: Vec3,
    pub(crate) team: Option<Team>,
    pub(crate) step: Num,
    pub(crate) walker: Walker,
    pub(crate) stuck: bool,
    pub(crate) gathering: bool,
}

/// A body of this tick as steering reads it: the unit's body where it stands, its team, whether
/// the unit walks this tick, whether it is in a gather loop, and whether it stands idle: a unit
/// that walks by a destination, with none, nothing under way, no target and no work that holds
/// it, which a destination may move aside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TickBody {
    pub(crate) body: IndexedBody,
    pub(crate) team: Option<Team>,
    pub(crate) walks: bool,
    pub(crate) gathers: bool,
    pub(crate) idle: bool,
}

/// A unit that steps aside for a stuck walker, and the point it steps to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Aside {
    pub(crate) id: StableId,
    pub(crate) to: Vec3,
}

/// A short route a walker takes: in place of the next `skipped` waypoints of its route, and
/// whether it ends on the goal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Detour {
    pub(crate) skipped: usize,
    pub(crate) reached: bool,
}

impl Steering {
    /// The cells a short route's window reaches from the walker's along each axis: room to go
    /// round a unit some cells wide on either side, in a search of at most 17 × 17 cells.
    pub(crate) const WINDOW: usize = 8;

    /// Reads this tick's `bodies`, in one pass: those of the units that stand into an index with
    /// the buckets of `statics`, and those of the units that walk into a grid; the units in a
    /// gather loop, whose standing bodies a second index leaves out.
    pub(crate) fn read(&mut self, statics: &BodyIndex, bodies: impl IntoIterator<Item = TickBody>) {
        self.still.clear();
        self.moving.clear();
        self.gatherers.clear();
        self.idle.clear();
        for TickBody {
            body,
            team,
            walks,
            gathers,
            idle,
        } in bodies
        {
            if walks {
                self.moving.push(Indexed {
                    id: body.id,
                    key: body.layer,
                    body: Placed::new(body.position, body.shape),
                });
            } else {
                self.still.push(body);
            }
            if gathers {
                self.gatherers.push(body.id);
            }
            if let Some(team) = team.filter(|_| idle && !walks && !gathers) {
                self.idle.push((body.id, team));
            }
        }
        self.idle.sort_unstable_by_key(|&(id, _)| id);
        self.still.sort_unstable_by_key(|body| body.id);
        let standing = self.standing.get_or_insert_with(|| statics.sibling());
        standing.update(&self.still);
        self.walking.rebuild(self.moving.iter().copied());
        self.gatherers.sort_unstable();
        if self.gatherers.is_empty() {
            return;
        }
        let gatherers = &self.gatherers;
        self.still_apart.clear();
        self.still_apart.extend(
            self.still
                .iter()
                .filter(|body| gatherers.binary_search(&body.id).is_err()),
        );
        let apart = self.apart.get_or_insert_with(|| statics.sibling());
        apart.update(&self.still_apart);
    }

    /// Puts in `asides` each unit of its team that stands idle ahead of `steered`, a stuck walker
    /// toward `ahead`, and that it touches, with the point it steps to: off the walker's way, on the side it
    /// stands on, or to the walker's left when it stands on the way, as walkers pass on the right,
    /// until it clears the walker's body, as Zero Hour tells a unit in the way to move away and
    /// OpenRA nudges an idle friend that blocks a move (04-capabilities/navigation.md, An idle
    /// friend steps aside). One whose side clears the walker already stays.
    pub(crate) fn asides(&self, steered: Steered, ahead: Vec3, asides: &mut Vec<Aside>) {
        let Steered {
            position: at,
            team: Some(team),
            step,
            walker,
            ..
        } = steered
        else {
            return;
        };
        let Some(forward) = at.ground_offset(ahead).normalized() else {
            return;
        };
        let left = Vec3::new(-forward.z, Num::ZERO, forward.x);
        let standing = self.standing.as_ref().expect("steering read the bodies");
        let idle = &self.idle;
        standing.near(walker.layer, at, walker.radius + step, |body| {
            let clear = walker.radius + body.shape.bound();
            let friend = idle
                .binary_search_by_key(&body.id, |&(id, _)| id)
                .is_ok_and(|place| idle[place].1 == team);
            if !friend || !body.position.within_ground(at, clear + step) {
                return;
            }
            // One behind the walker, or beside it, is not in its way.
            let off_walker = at.ground_offset(body.position);
            if off_walker.dot(forward) <= Num::ZERO {
                return;
            }
            let side = off_walker.dot(left);
            let (toward, off) = if side < Num::ZERO {
                (-left, -side)
            } else {
                (left, side)
            };
            if off >= clear {
                return;
            }
            asides.push(Aside {
                id: body.id,
                to: body.position + toward * (clear - off),
            });
        });
    }

    /// Steers `steered` along `route`, which has a waypoint ahead, on `grid` round `statics`: the
    /// detour it takes, by `short`, if any. One whose next stretch, as far as its window reaches,
    /// would overlap a unit of its layer that stands plans a short route in the window, with the
    /// units that stand there as blockers; so does one that is stuck, which marks as blockers
    /// the walkers it touches too. The short route goes to the last waypoint within the window,
    /// or to where the way leaves it, and the walker goes on from there; one that cannot end
    /// there keeps its route. One that `planner` has no work left for this tick keeps its route.
    pub(crate) fn steer(
        &mut self,
        planner: &mut RoutePlanner,
        grid: &PathingGrid,
        statics: &BodyIndex,
        steered: Steered,
        ahead: &[Vec3],
    ) -> Option<Detour> {
        let Steered {
            id,
            position: at,
            step,
            walker,
            stuck,
            gathering,
            ..
        } = steered;
        let standing = match (gathering && !self.gatherers.is_empty(), &self.apart) {
            (true, Some(apart)) => apart,
            _ => self.standing.as_ref().expect("steering read the bodies"),
        };
        let window_cells = i64::try_from(Self::WINDOW).expect("a small window");
        let reach = grid.cell() * window_cells;
        let look = at.step_toward(ahead[0], reach);
        if (!stuck && !standing.blocks(Segment::new(at, look), walker)) || planner.is_spent() {
            return None;
        }
        let clearance = grid.clearance(walker);
        let cells = clearance.grid();
        let window = Window::around(cells, cells.nearest_cell(at), Self::WINDOW);
        let inside = |position: &Vec3| {
            let cell = cells.nearest_cell(*position);
            window.contains(cell % cells.columns(), cell / cells.columns())
        };
        let within = ahead.iter().take_while(|position| inside(position)).count();
        let (goal, skipped) = match within {
            0 => (look, 0),
            _ => (ahead[within - 1], within),
        };
        let last = skipped == ahead.len();
        self.blockers.clear();
        // The window's centers lie up to its half and half a cell from `at` along each axis, and
        // a body blocks one it comes closer to than the two radii.
        let half_cell = grid.cell() / 2;
        let near = reach + half_cell + walker.radius;
        standing.near(walker.layer, at, near, |body| {
            self.blockers.push(*body);
        });
        let way = at.ground_offset(goal);
        let left = Vec3::new(-way.z, Num::ZERO, way.x).normalized();
        if let (true, Some(left)) = (stuck, left) {
            // A walker that keeps this one back counts as standing half their reach to this one's
            // left, so this one goes round it on its right; two that meet head on so pass on
            // opposite sides, whatever the cells make of their sides.
            let gatherers = &self.gatherers;
            self.walking.visit_near(at, walker.radius + step, |other| {
                let reach = walker.radius + other.body.shape.bound();
                let touching = other.body.position.within_ground(at, reach + step);
                let passes = gathering && gatherers.binary_search(&other.id).is_ok();
                if other.id == id || !touching || other.key != walker.layer || passes {
                    return ControlFlow::Continue(());
                }
                let shift = left * (reach / 2);
                self.blockers.push(IndexedBody {
                    id: other.id,
                    position: other.body.position + shift,
                    shape: other.body.shape,
                    layer: other.key,
                });
                ControlFlow::Continue(())
            });
        }
        let walkable = Walkable {
            clearance,
            statics,
            short: Some(Short {
                window,
                blockers: &self.blockers,
            }),
        };
        let outcome = planner.plan(walkable, at, goal, &mut self.short);
        // A plan with no cell to stand in would splice nothing in, and the walker would drop its
        // destination; it keeps its route, and steers again.
        let detour = Detour {
            skipped,
            reached: outcome.reached,
        };
        (!self.short.is_empty() && (outcome.reached || last)).then_some(detour)
    }

    /// The short route of the last detour.
    pub(crate) fn short(&self) -> &[Vec3] {
        &self.short
    }
}

#[cfg(test)]
mod tests;
