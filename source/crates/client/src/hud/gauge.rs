use bevy::ecs::component::Component;
use bevy::ecs::entity::Entity;
use bevy::math::{Quat, Vec3};
use bevy::transform::components::Transform;
use campfire_capabilities::PoolId;
use campfire_math::{Num, Tick, Ticks};

use crate::view;

/// A bar over a unit: its life, or, for the player's own avatar, another pool or the cooldown of
/// one ability slot. It draws with two children: a back of its full width, and `fill`.
#[derive(Component, Debug)]
pub(crate) struct Gauge {
    pub(crate) unit: Entity,
    pub(crate) kind: GaugeKind,
    pub(crate) fill: Entity,
}

/// What a gauge shows, and what it remembers to show it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GaugeKind {
    /// The unit's life pool; `shown` is the life it showed last, so a drop marks a hit.
    Life { shown: Option<Num> },
    /// Another pool of the unit, in row `row` of the stack.
    Pool { pool: PoolId, row: u8 },
    /// The cooldown of ability slot `slot`, in row `row`; `cooling` is the cooldown in view, once
    /// one starts.
    Cooldown {
        slot: u8,
        row: u8,
        cooling: Option<Cooling>,
    },
}

/// A cooldown as the client saw it start: the tick the ability is ready again, and the tick the
/// client first saw that, from which the gauge fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Cooling {
    pub(crate) ready_at: Tick,
    pub(crate) seen_at: Tick,
}

/// A bar's thickness, and the gap between rows.
const THICKNESS: f32 = 0.12;
const ROW: f32 = 0.16;
/// The pool bars' width.
const WIDE: f32 = 1.2;
/// A cooldown pip's width, and the step from one pip to the next.
const PIP: f32 = 0.27;
const PIP_STEP: f32 = 0.31;

impl GaugeKind {
    /// The bar's width, and its center from the top of the stack, in the plane that faces the
    /// camera: x across, z down the stack.
    pub(crate) fn layout(self) -> Layout {
        match self {
            GaugeKind::Life { .. } => Layout {
                width: WIDE,
                center: Vec3::ZERO,
            },
            GaugeKind::Pool { row, .. } => Layout {
                width: WIDE,
                center: Vec3::new(0.0, 0.0, f32::from(row) * ROW),
            },
            GaugeKind::Cooldown { slot, row, .. } => Layout {
                width: PIP,
                center: Vec3::new(
                    (f32::from(slot) - 1.5) * PIP_STEP,
                    0.0,
                    f32::from(row) * ROW,
                ),
            },
        }
    }
}

/// Where a bar sits in its stack, and how wide it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Layout {
    pub(crate) width: f32,
    pub(crate) center: Vec3,
}

impl Layout {
    /// The back's transform, relative to the gauge.
    pub(crate) fn back(self) -> Transform {
        Transform::from_scale(Vec3::new(self.width, 1.0, THICKNESS))
    }

    /// The fill's transform, relative to the gauge, for `fraction` of the bar: from the left edge,
    /// and just in front of the back.
    pub(crate) fn fill(self, fraction: f32) -> Transform {
        let fraction = fraction.clamp(0.0, 1.0);
        let left = -(1.0 - fraction) * self.width / 2.0;
        Transform::from_translation(Vec3::new(left, 0.01, 0.0)).with_scale(Vec3::new(
            fraction * self.width,
            1.0,
            THICKNESS,
        ))
    }

    /// The gauge's transform over a unit whose top is at `top`, facing the camera with `facing`.
    pub(crate) fn place(self, top: Vec3, facing: Quat) -> Transform {
        Transform::from_translation(top + facing * self.center).with_rotation(facing)
    }
}

impl Cooling {
    /// How far the gauge has filled at tick `now`: 0 when the client saw the cooldown start, 1
    /// once the ability is ready.
    #[expect(
        clippy::cast_precision_loss,
        reason = "a cooldown lasts far fewer ticks than an f32 counts exactly"
    )]
    pub(crate) fn filled(self, now: Tick) -> f32 {
        let total = self.ready_at.since(self.seen_at).map_or(0, Ticks::get);
        if total == 0 {
            return 1.0;
        }
        let elapsed = now.since(self.seen_at).map_or(0, Ticks::get).min(total);
        elapsed as f32 / total as f32
    }
}

/// `current` of `max`, as a share from 0 to 1; 0 when the maximum is not positive.
pub(crate) fn share(current: Num, max: Num) -> f32 {
    if max <= Num::ZERO {
        return 0.0;
    }
    let ratio = current.checked_div(max).unwrap_or(Num::ZERO);
    view::float(ratio).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bar_fills_from_its_left_edge_and_a_pip_from_where_its_cooldown_was_seen() {
        // A 1.2 m bar at a quarter: 0.3 m wide, its center 0.45 m left of the bar's.
        let health = GaugeKind::Life { shown: None }.layout();
        let quarter = health.fill(0.25);
        assert_eq!(quarter.scale, Vec3::new(0.3, 1.0, THICKNESS));
        assert!((quarter.translation.x + 0.45).abs() < 1e-6, "{quarter:?}");
        // Out of range, a share holds at the ends.
        assert_eq!(health.fill(1.5).scale.x, WIDE);
        assert_eq!(health.fill(-1.0).scale.x, 0.0);
        // The pips of slots 0 and 3 sit 1.5 steps either side, two rows down.
        let pip = |slot| {
            GaugeKind::Cooldown {
                slot,
                row: 2,
                cooling: None,
            }
            .layout()
            .center
        };
        assert_eq!(pip(0), Vec3::new(-1.5 * PIP_STEP, 0.0, 2.0 * ROW));
        assert_eq!(pip(3), Vec3::new(1.5 * PIP_STEP, 0.0, 2.0 * ROW));
        // Seen in tick 100, ready in 190: empty then, half in 145, full from 190.
        let cooling = Cooling {
            ready_at: Tick::new(190),
            seen_at: Tick::new(100),
        };
        let filled = |tick| cooling.filled(Tick::new(tick));
        assert_eq!(
            [filled(100), filled(145), filled(190), filled(400)],
            [0.0, 0.5, 1.0, 1.0]
        );
        // Seen only once ready: full.
        let ready = Cooling {
            ready_at: Tick::new(50),
            seen_at: Tick::new(60),
        };
        assert_eq!(ready.filled(Tick::new(60)), 1.0);
    }

    #[test]
    fn a_share_is_current_over_max_within_zero_and_one() {
        let num = |value| Num::from_int(value).unwrap();
        assert_eq!(share(num(150), num(600)), 0.25);
        assert_eq!(share(num(700), num(600)), 1.0);
        assert_eq!(share(num(5), Num::ZERO), 0.0);
        assert_eq!(share(Num::ZERO, num(600)), 0.0);
    }
}
