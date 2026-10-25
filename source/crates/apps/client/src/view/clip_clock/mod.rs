use campfire_client_files::AnimationMode;

/// Where a state's clip stands, as W3D's `Animatable3DObjClass::Compute_Current_Frame` moves its
/// frame: its frame, from 0 to its last, `frames − 1`; its mode; and the way it plays, forwards or
/// backwards, which a ping-pong turns at each end (animated-looks.md, A state plays its animation
/// by its mode).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ClipClock {
    frame: f32,
    last: f32,
    mode: AnimationMode,
    backwards: bool,
}

/// The frame a clip starts at, as `W3DModelDraw::adjustAnimation` picks it: its mode's own, its
/// first for a forward mode and its last for a backward one; a frame of its own, as a random one;
/// its first; its last; or the share of its frames the state before it stood at, kept across
/// states.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ClipStart {
    Natural,
    At(u32),
    First,
    Last,
    Kept(f32),
}

impl ClipClock {
    /// The clock of a clip of `frames` frames, played by `mode`, at `start`. A kept share starts
    /// at `share · frames − 1`, truncated, as `adjustAnimation` computes it, within the clip.
    pub(crate) fn start(frames: u32, mode: AnimationMode, start: ClipStart) -> Self {
        debug_assert!(frames > 0, "a clip has a frame");
        let last = (frames - 1) as f32;
        let backwards = matches!(
            mode,
            AnimationMode::LoopBackwards | AnimationMode::OnceBackwards
        );
        let frame = match start {
            ClipStart::Natural if backwards => last,
            ClipStart::Natural | ClipStart::First => 0.0,
            ClipStart::Last => last,
            ClipStart::At(frame) => (frame as f32).min(last),
            ClipStart::Kept(share) => {
                let frame = (share * frames as f32 - 1.0).trunc();
                frame.clamp(0.0, last)
            }
        };
        Self {
            frame,
            last,
            mode,
            backwards,
        }
    }

    pub(crate) const fn frame(&self) -> f32 {
        self.frame
    }

    /// The share of its frames it stands at, as `getCurrentAnimFraction` gives it: its frame of
    /// its last, 0 for a clip of one frame.
    pub(crate) fn share(&self) -> f32 {
        if self.last > 0.0 {
            self.frame / self.last
        } else {
            0.0
        }
    }

    /// Whether it played to its end, of a mode that plays once, as `Is_Animation_Complete` says:
    /// its last frame forwards, its first backwards.
    pub(crate) fn is_complete(&self) -> bool {
        match self.mode {
            AnimationMode::Once => self.frame == self.last,
            AnimationMode::OnceBackwards => self.frame == 0.0,
            AnimationMode::Manual
            | AnimationMode::Loop
            | AnimationMode::PingPong
            | AnimationMode::LoopBackwards => false,
        }
    }

    /// Moves it by `by` frames, its rate times its speed times the time passed: a once mode
    /// stops at its end, a loop wraps at its last frame, a ping-pong turns there, and a manual
    /// clip holds.
    pub(crate) fn advance(&mut self, by: f32) {
        if self.mode == AnimationMode::Manual {
            return;
        }
        let last = self.last;
        let mut frame = if self.backwards {
            self.frame - by
        } else {
            self.frame + by
        };
        match self.mode {
            AnimationMode::Manual => {}
            AnimationMode::Once => frame = frame.min(last),
            AnimationMode::OnceBackwards => frame = frame.max(0.0),
            AnimationMode::Loop => {
                if frame >= last {
                    frame -= last;
                    if frame >= last {
                        frame = 0.0;
                    }
                }
            }
            AnimationMode::LoopBackwards => {
                if frame < 0.0 {
                    frame += last;
                    if frame < 0.0 {
                        frame = last;
                    }
                }
            }
            AnimationMode::PingPong => {
                if !self.backwards && frame >= last {
                    frame = 2.0 * last - frame;
                    if frame >= last - 1.0 {
                        frame = last;
                    }
                    self.backwards = true;
                } else if self.backwards && frame < 0.0 {
                    frame = -frame;
                    if frame >= last {
                        frame = 0.0;
                    }
                    self.backwards = false;
                }
            }
        }
        self.frame = frame;
    }
}

#[cfg(test)]
mod tests;
