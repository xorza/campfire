use std::collections::VecDeque;
use std::time::Duration;

use bevy::ecs::entity::Entity;
use bevy::math::Vec3;
use campfire_client_files::TrackMarks;

use crate::view::effects::particle_quads::GroupQuads;

/// The tracks the match's treads lay (02-engine-core.md, A tread lays track marks): each a strip of
/// edges its drawing left behind it as it moved, laid, faded and dropped as the effects file's
/// marks say, as `W3DTerrainTracks` lays them, and the quads of each of their textures' groups.
#[derive(Debug)]
pub(crate) struct TrackRun {
    marks: TrackMarks,
    tracks: Vec<Track>,
    groups: Vec<GroupQuads>,
}

/// A track: the drawing that lays it while it is bound, its texture's group, its width, the
/// point its next edge is measured from, whether its next edge starts it again, how many edges
/// it laid, and its edges, the oldest first.
#[derive(Debug)]
struct Track {
    root: Option<Entity>,
    group: usize,
    width: f32,
    anchor: Option<Vec3>,
    restarts: bool,
    laid: u32,
    edges: VecDeque<Edge>,
}

/// An edge of a track: its left and its right ends, its texture's `v`, when it was laid, and its
/// alpha.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Edge {
    ends: [Vec3; 2],
    v: f32,
    born: Duration,
    alpha: f32,
}

impl TrackRun {
    /// No track yet, of the marks `marks`, in `groups` groups of textures.
    pub(crate) fn new(marks: TrackMarks, groups: usize) -> Self {
        Self {
            marks,
            tracks: Vec::new(),
            groups: vec![GroupQuads::default(); groups],
        }
    }

    /// Binds a track of the group `group`, `width` wide, to the drawing `root`, unless as many
    /// tracks as the marks hold are laid, as `bindTrack` binds none from an empty store.
    pub(crate) fn bind(&mut self, root: Entity, group: usize, width: f32) {
        if self.tracks.len() >= usize::from(self.marks.tracks) {
            return;
        }
        self.tracks.push(Track {
            root: Some(root),
            group,
            width,
            anchor: None,
            restarts: true,
            laid: 0,
            edges: VecDeque::with_capacity(usize::from(self.marks.edges)),
        });
    }

    /// Lays an edge of each bound track where `place` gives its drawing stands, at `now` of app
    /// time, over a ground of `ground`'s heights, unbinding the track of a drawing `place` gives
    /// none of; fades each edge by its age and drops those faded out, then each unbound track of
    /// no edge left (`TerrainTracksRenderObjClassSystem::update`).
    pub(crate) fn lay(
        &mut self,
        now: Duration,
        ground: &impl Fn(f32, f32) -> f32,
        place: impl Fn(Entity) -> Option<Vec3>,
    ) {
        let marks = self.marks;
        let fade = Duration::from_secs_f32(marks.fade);
        for track in &mut self.tracks {
            if let Some(root) = track.root {
                match place(root) {
                    Some(at) => track.lay(at, now, ground, marks),
                    None => track.root = None,
                }
            }
            while track
                .edges
                .front()
                .is_some_and(|edge| now.saturating_sub(edge.born) >= fade)
            {
                track.edges.pop_front();
            }
            for edge in &mut track.edges {
                if edge.alpha > 0.0 {
                    edge.alpha = 1.0 - now.saturating_sub(edge.born).as_secs_f32() / marks.fade;
                }
            }
        }
        self.tracks
            .retain(|track| track.root.is_some() || !track.edges.is_empty());
    }

    /// Writes each group's quads of its tracks of two edges or more: two vertices an edge, on
    /// its ends, of the texture's width across and its `v` along, white of its alpha times its
    /// fade by its place, the edges past the newest opaque ones fading toward the oldest, and two
    /// triangles between each edge and the next, counter-clockwise seen from above
    /// (`TerrainTracksRenderObjClassSystem::flush`).
    pub(crate) fn build(&mut self) {
        for group in &mut self.groups {
            group.positions.clear();
            group.uvs.clear();
            group.colors.clear();
            group.indices.clear();
        }
        let marks = self.marks;
        let fading = f32::from(marks.edges - marks.opaque);
        for track in &self.tracks {
            let count = track.edges.len();
            if count < 2 {
                continue;
            }
            let group = &mut self.groups[track.group];
            let first = u32::try_from(group.positions.len()).expect("a group's vertices fit u32");
            for (at, edge) in track.edges.iter().enumerate() {
                let newer = count - 1 - at;
                let place = if newer >= usize::from(marks.opaque) {
                    let past =
                        u16::try_from(count - at).expect("a track's edges fit u16") - marks.opaque;
                    1.0 - f32::from(past) / fading
                } else {
                    1.0
                };
                let alpha = place * edge.alpha;
                for (end, u) in edge.ends.iter().zip([0.0, 1.0]) {
                    group.positions.push(end.to_array());
                    group.uvs.push([u, edge.v]);
                    group.colors.push([1.0, 1.0, 1.0, alpha]);
                }
            }
            let edges = u32::try_from(count).expect("a track's edges fit u32");
            for at in 0..edges - 1 {
                let left = first + 2 * at;
                let [right, next_left, next_right] = [left + 1, left + 2, left + 3];
                group
                    .indices
                    .extend_from_slice(&[left, right, next_left, next_left, right, next_right]);
            }
        }
    }

    /// The quads of the group `group`.
    pub(crate) fn group(&self, group: usize) -> &GroupQuads {
        &self.groups[group]
    }
}

impl Track {
    /// Lays an edge where its drawing stands at `at`, at `now`, over a ground of `ground`'s
    /// heights, by `marks`, once it moved the marks' spacing from its last
    /// (`TerrainTracksRenderObjClass::addEdgeToTrack`): the first place anchors it and lays none;
    /// each edge stands across its move, `width` wide, each end the marks' lift over the drawn
    /// ground under it; the oldest goes past the marks' edges; its texture's `v` alternates from
    /// 1; an edge that starts the track again, and its second, lie transparent.
    fn lay(
        &mut self,
        at: Vec3,
        now: Duration,
        ground: &impl Fn(f32, f32) -> f32,
        marks: TrackMarks,
    ) {
        let center = Vec3::new(at.x, ground(at.x, at.z), at.z);
        let Some(anchor) = self.anchor else {
            self.anchor = Some(center);
            self.restarts = true;
            return;
        };
        let moved = center - anchor;
        if moved.length_squared() < marks.spacing * marks.spacing {
            return;
        }
        if self.edges.len() >= usize::from(marks.edges) {
            self.edges.pop_front();
        }
        let way = Vec3::new(moved.x, 0.0, moved.z).normalize_or_zero();
        let side = way.cross(Vec3::Y) * (self.width * 0.5);
        let end = |point: Vec3| Vec3::new(point.x, ground(point.x, point.z) + marks.lift, point.z);
        let alpha = if self.restarts || self.edges.len() <= 1 {
            0.0
        } else {
            1.0
        };
        self.edges.push_back(Edge {
            ends: [end(center - side), end(center + side)],
            v: if self.laid % 2 == 1 { 0.0 } else { 1.0 },
            born: now,
            alpha,
        });
        self.restarts = false;
        self.anchor = Some(center);
        self.laid += 1;
    }
}

#[cfg(test)]
mod tests;
