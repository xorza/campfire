use bevy_math::Vec2;

/// A convex polygon clipped to one side of a line after another (Sutherland–Hodgman): a corner
/// on the line stays, and an edge whose ends lie strictly on both sides gains a corner where it
/// crosses, so a clip repeats no corner. Its buffers are the scratch of each polygon in turn.
#[derive(Debug)]
pub(crate) struct ConvexClip<T> {
    polygon: Vec<T>,
    clipped: Vec<T>,
}

impl<T: Copy> ConvexClip<T> {
    /// Starts a polygon of `corners`.
    pub(crate) fn start(&mut self, corners: &[T]) {
        self.polygon.clear();
        self.polygon.extend_from_slice(corners);
    }

    /// Clips the polygon to where `side` is at least 0, each crossing `cross` of an edge's ends
    /// and its share of the way from the first.
    pub(crate) fn clip(&mut self, side: impl Fn(T) -> f32, cross: impl Fn(T, T, f32) -> T) {
        self.clipped.clear();
        let count = self.polygon.len();
        for at in 0..count {
            let [from, to] = [self.polygon[at], self.polygon[(at + 1) % count]];
            let [start, end] = [from, to].map(&side);
            if start >= 0.0 {
                self.clipped.push(from);
            }
            if (start > 0.0 && end < 0.0) || (start < 0.0 && end > 0.0) {
                self.clipped.push(cross(from, to, start / (start - end)));
            }
        }
        std::mem::swap(&mut self.polygon, &mut self.clipped);
    }

    /// The polygon's corners, in their order.
    pub(crate) fn corners(&self) -> &[T] {
        &self.polygon
    }

    /// Whether the polygon, its corners at `point`, has an area.
    pub(crate) fn has_area(&self, point: impl Fn(T) -> Vec2) -> bool {
        let count = self.polygon.len();
        let area: f32 = (0..count)
            .map(|at| point(self.polygon[at]).perp_dot(point(self.polygon[(at + 1) % count])))
            .sum();
        count >= 3 && area != 0.0
    }
}

impl<T> Default for ConvexClip<T> {
    fn default() -> Self {
        Self {
            polygon: Vec::new(),
            clipped: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cross(from: Vec2, to: Vec2, share: f32) -> Vec2 {
        from + (to - from) * share
    }

    #[test]
    fn a_clip_keeps_the_corners_on_its_line_once_and_adds_each_strict_crossing() {
        // The triangle (0, 0), (10, 0), (0, 10) right of x = 5: (10, 0) stays, the edge from
        // (0, 0) crosses at (5, 0), and the edge from (10, 0) to (0, 10) at (5, 5).
        let mut clip = ConvexClip::default();
        let triangle = [Vec2::ZERO, Vec2::new(10.0, 0.0), Vec2::new(0.0, 10.0)];
        clip.start(&triangle);
        clip.clip(|point| point.x - 5.0, cross);
        assert_eq!(
            clip.corners(),
            [
                Vec2::new(5.0, 0.0),
                Vec2::new(10.0, 0.0),
                Vec2::new(5.0, 5.0)
            ]
        );
        assert!(clip.has_area(|point| point));

        // Left of x = 0, its corners (0, 0) and (0, 10) lie on the line, and the edges to and
        // from (10, 0) reach it only there: the two corners stay, each once, of no area.
        clip.start(&triangle);
        clip.clip(|point| -point.x, cross);
        assert_eq!(clip.corners(), [Vec2::ZERO, Vec2::new(0.0, 10.0)]);
        assert!(!clip.has_area(|point| point));

        // Right of x = 0, the triangle stays whole.
        clip.start(&triangle);
        clip.clip(|point| point.x, cross);
        assert_eq!(clip.corners(), triangle);
    }
}
