use std::cmp::Ordering;
use std::ops::{ControlFlow, Range};

use campfire_geometry::{BodyGrid, Indexed, Placed};
use campfire_math::{Num, Vec3};
use campfire_sim::StableId;
use campfire_units::Layer;
use campfire_values::Team;

use crate::aside_depth::AsideDepth;
use crate::body_index::{BodyIndex, IndexedBody};
use crate::collider::Collider;
use crate::moves::Moves;
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
    /// The units that stand idle, by stable id, with their teams and the goals they arrived at:
    /// those a walker that may move them moves aside, and those a walker bound for the same goal
    /// stops against.
    idle: Vec<Idle>,
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
    /// The widest bound of a unit that stands, which a search for the personal spaces round a
    /// point reaches past.
    widest: Num,
    /// The short route of the walker that steers now.
    short: Vec<Vec3>,
}

/// A walker that may steer this tick: its unit, where it stands, its team, if any, its step, its
/// kind, whether walkers keep it back, whether it gathers, which passes through the others that
/// do, its route's goal, and how deep its walk is in a chain of steps aside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Steered {
    pub id: StableId,
    pub position: Vec3,
    pub(crate) team: Option<Team>,
    pub(crate) step: Num,
    pub(crate) walker: Walker,
    pub(crate) stuck: bool,
    pub(crate) gathering: bool,
    pub(crate) goal: Vec3,
    pub(crate) depth: AsideDepth,
}

/// A unit that stands idle, of a team: its stable id, its team and the goal it arrived at, if
/// any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Idle {
    id: StableId,
    team: Team,
    arrived: Option<Vec3>,
}

/// A body of this tick as steering reads it: the unit's body where it stands, its team, whether
/// the unit walks this tick, whether it is in a gather loop, the goal it arrived at, and whether
/// it stands idle: a unit that walks by a destination, with none, nothing under way, no target
/// and no work that holds
/// it, which a destination may move aside.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TickBody {
    pub(crate) body: IndexedBody,
    pub(crate) team: Option<Team>,
    pub(crate) walks: bool,
    pub(crate) gathers: bool,
    pub(crate) idle: bool,
    pub(crate) arrived: Option<Vec3>,
}

/// A unit that steps aside for a walker, the point it steps to off the walker's way, how deep the
/// walker's walk is, the points of the walker's passage in the tick's buffer of them, and the
/// walker's radius.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Aside {
    pub(crate) id: StableId,
    pub(crate) to: Vec3,
    pub(crate) depth: AsideDepth,
    pub(crate) passage: Range<u32>,
    pub(crate) radius: Num,
}

/// What a unit that steps aside makes of the idle friends near it: places it may pass through
/// and end on, as each steps aside in turn for its walk, of `team`, which `moves` lets it move;
/// or blockers, as its walk is too deep to move them (04-capabilities/navigation.md, A friend
/// yields through the friends that yield).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Friends {
    Pass { team: Option<Team>, moves: Moves },
    Block,
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

    /// The cells a friend's search for a place to step aside to reaches from its cell along each
    /// axis: as many as the walker's passage reaches from the walker, its window and a reach past
    /// it, so the search of a friend in a corridor reaches the room past the corridor's end that
    /// the short route's window misses (04-capabilities/navigation.md, A friend yields through the
    /// friends that yield).
    pub(crate) const YIELD_WINDOW: usize = 2 * Self::WINDOW;

    /// How far a short route's window reaches from the walker's cell along each axis on `grid`.
    pub(crate) fn reach(grid: &PathingGrid) -> Num {
        grid.cell() * i64::try_from(Self::WINDOW).expect("a small window")
    }

    /// Reads this tick's `bodies`, in one pass: those of the units that stand into an index with
    /// the buckets of `statics`, and those of the units that walk into a grid; the units in a
    /// gather loop, whose standing bodies a second index leaves out.
    pub(crate) fn read(&mut self, statics: &BodyIndex, bodies: impl IntoIterator<Item = TickBody>) {
        self.still.clear();
        self.moving.clear();
        self.gatherers.clear();
        self.idle.clear();
        self.widest = Num::ZERO;
        for TickBody {
            body,
            team,
            walks,
            gathers,
            idle,
            arrived,
        } in bodies
        {
            if walks {
                self.moving.push(Indexed {
                    id: body.id,
                    key: body.layer,
                    body: Placed::new(body.position, body.shape),
                });
            } else {
                self.widest = self.widest.max(body.shape.bound());
                self.still.push(body);
            }
            if gathers {
                self.gatherers.push(body.id);
            }
            if let Some(team) = team.filter(|_| idle && !walks && !gathers) {
                self.idle.push(Idle {
                    id: body.id,
                    team,
                    arrived,
                });
            }
        }
        self.idle.sort_unstable_by_key(|idle| idle.id);
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

    /// Puts in `passages` the points of the way `steered` passes, which a friend that steps aside
    /// clears: where it stands, then the waypoints of `ahead` its window round it holds, then,
    /// past them, the point `reach` along the leg that leaves the window.
    pub(crate) fn passage(
        grid: &PathingGrid,
        steered: Steered,
        ahead: &[Vec3],
        passages: &mut Vec<Vec3>,
    ) {
        let at = steered.position;
        let clearance = grid.clearance(steered.walker);
        let cells = clearance.grid();
        let window = Window::around(cells, cells.nearest_cell(at), Self::WINDOW);
        passages.push(at);
        let mut from = at;
        for &point in ahead {
            let cell = cells.nearest_cell(point);
            if !window.contains(cell % cells.columns(), cell / cells.columns()) {
                passages.push(from.step_toward(point, Self::reach(grid)));
                return;
            }
            passages.push(point);
            from = point;
        }
    }

    /// Puts in `asides` each unit that stands idle on the next stretch of `steered`'s way, toward
    /// `ahead` and at most `reach` long, and that `moves` lets the walker move, with the point it
    /// steps to: off the walker's way, on the side it stands on, or to the walker's left when it
    /// stands on the way, as walkers pass on the right, until it clears the walker's body, as Zero
    /// Hour's pathfinder moves the allies off a new path and OpenRA nudges an idle friend that
    /// blocks a move (04-capabilities/navigation.md, An idle friend steps aside), with how deep
    /// the walker's walk is, and `passage`, the walker's points in the tick's buffer, which its
    /// place clears. One whose side clears the walker already stays, and a walk as deep as
    /// `AsideDepth::MOST` moves none (04-capabilities/navigation.md, An idle friend steps aside).
    pub(crate) fn asides(
        &self,
        steered: Steered,
        (ahead, reach): (Vec3, Num),
        moves: Moves,
        passage: &Range<u32>,
        asides: &mut Vec<Aside>,
    ) {
        let Steered {
            position: at,
            team,
            walker,
            goal,
            depth,
            ..
        } = steered;
        if !depth.clears() {
            return;
        }
        let way = at.ground_offset(ahead);
        let Some(forward) = way.normalized() else {
            return;
        };
        let stretch = way.length().min(reach);
        let left = Vec3::new(-forward.z, Num::ZERO, forward.x);
        let standing = self.standing.as_ref().expect("steering read the bodies");
        let idle = &self.idle;
        standing.near(walker.layer, at, stretch + walker.radius, |body| {
            let clear = walker.radius + body.shape.bound();
            // One that arrived at the walker's goal is of the crowd there, which the walker
            // stops against (04-capabilities/navigation.md, A crowded goal ends the walk).
            let movable = idle
                .binary_search_by_key(&body.id, |idle| idle.id)
                .is_ok_and(|place| {
                    let idle = idle[place];
                    moves.may_move(team, Some(idle.team)) && idle.arrived != Some(goal)
                });
            if !movable {
                return;
            }
            // One behind the walker, beside it, or past the stretch is not in its way.
            let off_walker = at.ground_offset(body.position);
            let along = off_walker.dot(forward);
            if along <= Num::ZERO || along >= stretch + clear {
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
                depth,
                passage: passage.clone(),
                radius: walker.radius,
            });
        });
    }

    /// Whether `steered` touches a unit of its team that stands idle, arrived at its goal, nearer
    /// the goal than it: one of the crowd there, so it has arrived too, as the A* Pathfinding
    /// Project stops a unit whose crowded goal it cannot reach (04-capabilities/navigation.md, A
    /// crowded goal ends the walk).
    pub(crate) fn packed(&self, steered: Steered) -> bool {
        let Steered {
            position: at,
            team: Some(team),
            step,
            walker,
            goal,
            ..
        } = steered
        else {
            return false;
        };
        let standing = self.standing.as_ref().expect("steering read the bodies");
        let idle = &self.idle;
        let off = at.ground_offset(goal).length_squared_bits();
        let mut packed = false;
        standing.near(walker.layer, at, walker.radius + step, |body| {
            let crowd = idle
                .binary_search_by_key(&body.id, |idle| idle.id)
                .is_ok_and(|place| idle[place].team == team && idle[place].arrived == Some(goal));
            let touches = body
                .position
                .within_ground(at, walker.radius + body.shape.bound() + step);
            packed |=
                crowd && touches && body.position.ground_offset(goal).length_squared_bits() < off;
        });
        packed
    }

    /// Whether `waypoint`, the last of `steered`'s route, lies within the personal space a unit
    /// that stands shares with `steered`, which stands within it too: a goal the unit crowds, as
    /// near as collision lets the walker come, so it has arrived, as the A* Pathfinding Project
    /// stops a unit whose destination is crowded (04-capabilities/navigation.md, A walker stuck by
    /// a crowded goal has arrived). Each space is collision's, `Collider::space_of`.
    pub(crate) fn crowded(&self, steered: Steered, waypoint: Vec3) -> bool {
        let Steered {
            position: at,
            walker,
            ..
        } = steered;
        let standing = self.standing.as_ref().expect("steering read the bodies");
        let own = Collider::space_of(walker.radius);
        let mut crowded = false;
        standing.near(
            walker.layer,
            at,
            own + Collider::space_of(self.widest),
            |body| {
                let space = own + Collider::space_of(body.shape.bound());
                let inside = |point: Vec3| {
                    point.ground().distance_order(body.position.ground(), space) == Ordering::Less
                };
                crowded |= inside(at) && inside(waypoint);
            },
        );
        crowded
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
        let reach = Self::reach(grid);
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
            crowd: None,
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

    /// Puts in `blockers` the units that stand near `body`, a unit that steps aside, as far as
    /// its yield window and its body reach on `grid`, but `body` itself and, as `friends` says, the
    /// idle friends it may pass through: the places it may not take.
    pub(crate) fn standing_near(
        &self,
        grid: &PathingGrid,
        body: IndexedBody,
        friends: Friends,
        blockers: &mut Vec<IndexedBody>,
    ) {
        let standing = self.standing.as_ref().expect("steering read the bodies");
        let window = grid.cell() * i64::try_from(Self::YIELD_WINDOW).expect("a small window");
        let near = window + grid.cell() / 2 + body.shape.bound();
        let idle = &self.idle;
        let passes = |other: &IndexedBody| match friends {
            Friends::Pass { team, moves } => idle
                .binary_search_by_key(&other.id, |idle| idle.id)
                .is_ok_and(|place| moves.may_move(team, Some(idle[place].team))),
            Friends::Block => false,
        };
        blockers.clear();
        standing.near(body.layer, body.position, near, |other| {
            if other.id != body.id && !passes(other) {
                blockers.push(*other);
            }
        });
    }

    /// The short route of the last detour.
    pub(crate) fn short(&self) -> &[Vec3] {
        &self.short
    }
}

#[cfg(feature = "internals")]
pub(crate) mod bench;

#[cfg(test)]
mod tests;
