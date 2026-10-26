use crate::zero_hour::format::ini::object_ini::ini_recoil::IniRecoil;
use crate::zero_hour::game_time;

/// A draw module's recoil of a barrel at its shot, by its draw frames, as `W3DModelDraw` holds
/// it: its speed at the shot a frame, the most it moves, and the share of its speed each frame
/// keeps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BarrelRecoil {
    speed: f32,
    most: f32,
    damping: f32,
}

impl BarrelRecoil {
    /// A frame's speed of `W3DModelDrawModuleData` when its lines give none, `INITIAL_RECOIL_RATE`.
    const SPEED: f32 = 2.0;

    /// The speed below which a recoil stops, `TINY_RECOIL`.
    const TINY: f32 = 0.01;

    /// The most draw frames a flash shows for, a minute's; a recoil that does not end within them
    /// shows none.
    const MOST_FRAMES: u32 = 60 * 30;

    /// The recoil `recoil` reads, its speed of a second turned into a frame's as
    /// `ConvertVelocityInSecsToFrames` turns it, by `SECONDS_PER_LOGICFRAME_REAL`, in the game's
    /// `f32`.
    pub(crate) fn of(recoil: IniRecoil) -> Self {
        let frame = 1.0 / game_time::FRAMES_PER_SECOND;
        Self {
            speed: recoil.speed.map_or(Self::SPEED, |speed| speed * frame),
            most: recoil.most,
            damping: recoil.damping,
        }
    }

    /// The draw frames a barrel's muzzle flash shows for after its shot, as
    /// `W3DModelDraw::handleClientRecoil` hides it each frame its recoil does not start: for a
    /// barrel of a recoil bone, until the recoil moves its most or its speed falls below
    /// `TINY`, each frame adding its speed and then damping it; for a barrel of none, one frame,
    /// as its recoil ends at its first; none for a recoil that does not end within
    /// `MOST_FRAMES`.
    pub(crate) fn flash_frames(self, recoils: bool) -> Option<u32> {
        if !recoils {
            return Some(1);
        }
        let (mut moved, mut speed) = (0.0, self.speed);
        for frame in 1..=Self::MOST_FRAMES {
            moved += speed;
            speed *= self.damping;
            if moved >= self.most || speed.abs() < Self::TINY {
                return Some(frame);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_flash_shows_while_its_barrels_recoil_starts() {
        // The defaults, 2 a frame: moved 2, 2.8, then 3.12, past 3, at the third frame.
        let default = BarrelRecoil::of(IniRecoil::default());
        assert_eq!(default.flash_frames(true), Some(3));
        // A barrel of no recoil bone: one frame, whatever its recoil.
        assert_eq!(default.flash_frames(false), Some(1));
        // 120 a second, 4 a frame, of a most of 8: moved 4, 5.6, 6.24, 6.496, 6.5984, 6.639,
        // 6.655, its speed then 0.0066, below 0.01, at the seventh.
        let fast = IniRecoil {
            speed: Some(120.0),
            most: 8.0,
            ..IniRecoil::default()
        };
        assert_eq!(BarrelRecoil::of(fast).flash_frames(true), Some(7));
        // A speed that never falls, back from its most: none; a speed of none stops at once.
        let back = IniRecoil {
            speed: Some(-30.0),
            most: 3.0,
            damping: 1.0,
        };
        assert_eq!(BarrelRecoil::of(back).flash_frames(true), None);
        let still = IniRecoil {
            speed: Some(0.0),
            ..IniRecoil::default()
        };
        assert_eq!(BarrelRecoil::of(still).flash_frames(true), Some(1));
    }
}
