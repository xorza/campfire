use std::collections::BTreeMap;

use crate::zero_hour::format::w3d::error::W3dError;
use crate::zero_hour::format::w3d::w3d_chunks::{W3dChunk, W3dChunks};
use crate::zero_hour::format::w3d::w3d_fields::W3dFields;

/// A W3D animation, as `HRawAnimClass` and `HCompressedAnimClass` load one: its name,
/// `<hierarchy>.<animation>` in lowercase, as `HAnimManager` finds it, its hierarchy's name, its
/// frames and their rate a second, and the channels of each pivot it moves, by the pivot's index.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct W3dAnimation {
    pub(crate) name: String,
    pub(crate) hierarchy: String,
    pub(crate) frames: u32,
    pub(crate) rate: u32,
    pub(crate) pivots: BTreeMap<usize, PivotChannels>,
}

/// A pivot's channels: its move along each of its parent's axes, its turn, and its visibility,
/// each the last of its kind the animation holds, as `NodeMotionStruct` keeps one.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct PivotChannels {
    pub(crate) moves: [Option<MotionKeys>; 3],
    pub(crate) turn: Option<MotionKeys>,
    pub(crate) visibility: Option<VisibilityChannel>,
}

/// A channel's keys, each a value of one float, a move, or of four, a turn's quaternion
/// `x, y, z, w`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum MotionKeys {
    Frames(FrameKeys),
    Timed(TimedKeys),
}

/// A key each frame from `first`, as `MotionChannelClass` holds them; a frame outside them is the
/// identity.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FrameKeys {
    first: u32,
    values: Vec<f32>,
}

/// Keys at their frames, from frame 0 up and none past the last, as `TimeCodedMotionChannelClass`
/// holds them: linear between two, the last held after it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TimedKeys {
    frames: Vec<u32>,
    values: Vec<f32>,
}

/// A visibility channel, as `BitChannelClass` holds it: a bit each frame from `first`, a bit of
/// a frame outside them `default`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VisibilityChannel {
    first: u32,
    count: u32,
    default: bool,
    bits: Vec<u8>,
}

/// What a channel's `Flags` say it moves, and the floats of each of its values.
#[derive(Debug, Clone, Copy)]
enum ChannelKind {
    Move(usize),
    Turn,
}

const ANIMATION: u32 = 0x0200;
const HEADER: u32 = 0x0201;
const CHANNEL: u32 = 0x0202;
const BIT_CHANNEL: u32 = 0x0203;
const COMPRESSED_ANIMATION: u32 = 0x0280;
const COMPRESSED_HEADER: u32 = 0x0281;
const COMPRESSED_CHANNEL: u32 = 0x0282;
const COMPRESSED_BIT_CHANNEL: u32 = 0x0283;

/// A name's bytes, `W3D_NAME_LEN`.
const NAME: usize = 16;
/// `W3D_MAKE_VERSION(3, 0)`: an older raw animation counts its pivots from the hierarchy's
/// first past its root, which `HRawAnimClass` adds 1 to.
const VERSION_3: u32 = 0x0003_0000;
/// The flavor of a compressed animation whose channels are time-coded, `ANIM_FLAVOR_TIMECODED`.
const TIMECODED: u16 = 0;
/// A visibility channel's `Flags`, `BIT_CHANNEL_VIS`; `HRawAnimClass` drops any other.
const VISIBILITY: u16 = 0;
/// The top bit of a time code, `W3D_TIMECODED_BINARY_MOVEMENT_FLAG`: the value holds until this
/// key, then steps to it.
const STEP: u32 = 0x8000_0000;

impl W3dAnimation {
    /// The chunk ids of an animation, raw and compressed, at a file's top.
    pub(crate) const CHUNKS: [u32; 2] = [ANIMATION, COMPRESSED_ANIMATION];

    /// The animation of `chunk`, of an id of `CHUNKS`.
    pub(crate) fn read(chunk: W3dChunk<'_>) -> Result<Self, W3dError> {
        if chunk.id == ANIMATION {
            Self::read_raw(chunk.body)
        } else {
            debug_assert_eq!(chunk.id, COMPRESSED_ANIMATION, "a chunk of an animation");
            Self::read_compressed(chunk.body)
        }
    }

    fn read_raw(body: &[u8]) -> Result<Self, W3dError> {
        let mut header = W3dFields::of(W3dChunks::of(body).first_of(HEADER)?.body);
        let version = header.cursor.u32()?;
        let mut animation = Self::named(&mut header)?;
        animation.frames = header.cursor.u32()?;
        animation.rate = header.cursor.u32()?;
        animation.check_timing()?;
        let shift = usize::from(version < VERSION_3);
        for chunk in W3dChunks::of(body) {
            let chunk = chunk?;
            let mut fields = W3dFields::of(chunk.body);
            match chunk.id {
                CHANNEL => {
                    let first = fields.cursor.u16()?;
                    let last = fields.cursor.u16()?;
                    let len = fields.cursor.u16()?;
                    let flags = fields.cursor.u16()?;
                    let pivot = usize::from(fields.cursor.u16()?) + shift;
                    fields.cursor.u16()?;
                    let kind = ChannelKind::of(flags, len)?;
                    let frames = channel_frames(first, last)?;
                    // The exporter wrote more than the frames, which the game skips.
                    let floats = usize::try_from(frames).expect("a u32 fits usize") * kind.len();
                    let values = finite_floats(&mut fields, floats)?;
                    let keys = MotionKeys::Frames(FrameKeys {
                        first: u32::from(first),
                        values,
                    });
                    animation.channel(pivot).set(kind, keys);
                }
                BIT_CHANNEL => {
                    let first = fields.cursor.u16()?;
                    let last = fields.cursor.u16()?;
                    let flags = fields.cursor.u16()?;
                    let pivot = usize::from(fields.cursor.u16()?) + shift;
                    let default = fields.cursor.u8()? != 0;
                    let count = channel_frames(first, last)?;
                    let bytes = usize::try_from(count.div_ceil(8)).expect("a u32 fits usize");
                    let bits = fields.cursor.take(bytes)?.to_vec();
                    if flags == VISIBILITY {
                        animation.channel(pivot).visibility = Some(VisibilityChannel {
                            first: u32::from(first),
                            count,
                            default,
                            bits,
                        });
                    }
                }
                _ => {}
            }
        }
        Ok(animation)
    }

    fn read_compressed(body: &[u8]) -> Result<Self, W3dError> {
        let mut header = W3dFields::of(W3dChunks::of(body).first_of(COMPRESSED_HEADER)?.body);
        header.cursor.u32()?;
        let mut animation = Self::named(&mut header)?;
        animation.frames = header.cursor.u32()?;
        animation.rate = u32::from(header.cursor.u16()?);
        let flavor = header.cursor.u16()?;
        if flavor != TIMECODED {
            return Err(W3dError::Flavor(flavor));
        }
        animation.check_timing()?;
        for chunk in W3dChunks::of(body) {
            let chunk = chunk?;
            let mut fields = W3dFields::of(chunk.body);
            match chunk.id {
                COMPRESSED_CHANNEL => {
                    let keys = fields.cursor.u32()?;
                    let pivot = usize::from(fields.cursor.u16()?);
                    let len = fields.cursor.u8()?;
                    let flags = fields.cursor.u8()?;
                    let kind = ChannelKind::of(u16::from(flags), u16::from(len))?;
                    let mut frames = Vec::new();
                    let mut values = Vec::new();
                    for at in 0..keys {
                        let code = fields.cursor.u32()?;
                        let frame = code & !STEP;
                        // `Get_Vector` reads a key's step as it reaches it, so the first's is
                        // never read.
                        if at > 0 && code & STEP != 0 {
                            return Err(W3dError::Step);
                        }
                        let rises = frames.last().map_or(frame == 0, |&last| frame > last);
                        if !rises || frame >= animation.frames {
                            return Err(W3dError::TimeCodes);
                        }
                        frames.push(frame);
                        values.extend(finite_floats(&mut fields, kind.len())?);
                    }
                    if frames.is_empty() {
                        return Err(W3dError::TimeCodes);
                    }
                    let keys = MotionKeys::Timed(TimedKeys { frames, values });
                    animation.channel(pivot).set(kind, keys);
                }
                COMPRESSED_BIT_CHANNEL => return Err(W3dError::TimedVisibility),
                _ => {}
            }
        }
        Ok(animation)
    }

    /// An animation of the names a header's `fields` hold next, of no frame and no channel yet.
    fn named(fields: &mut W3dFields<'_>) -> Result<Self, W3dError> {
        let name = fields.name(NAME)?;
        let hierarchy = fields.name(NAME)?;
        Ok(Self {
            name: format!("{hierarchy}.{name}"),
            hierarchy,
            frames: 0,
            rate: 0,
            pivots: BTreeMap::new(),
        })
    }

    const fn check_timing(&self) -> Result<(), W3dError> {
        if self.frames == 0 || self.rate == 0 {
            return Err(W3dError::Timing);
        }
        Ok(())
    }

    fn channel(&mut self, pivot: usize) -> &mut PivotChannels {
        self.pivots.entry(pivot).or_default()
    }
}

impl PivotChannels {
    /// Keeps `keys` as its channel of `kind`, in place of one it holds, as `add_channel` does.
    fn set(&mut self, kind: ChannelKind, keys: MotionKeys) {
        match kind {
            ChannelKind::Move(axis) => self.moves[axis] = Some(keys),
            ChannelKind::Turn => self.turn = Some(keys),
        }
    }
}

impl ChannelKind {
    /// The kind of a channel's `Flags`, `ANIM_CHANNEL_X` to `Z` and `Q`, of `len` floats a value;
    /// an error for any other, as the Euler turns, which no file of the install holds.
    fn of(flags: u16, len: u16) -> Result<Self, W3dError> {
        match (flags, len) {
            (0..=2, 1) => Ok(Self::Move(usize::from(flags))),
            (6, 4) => Ok(Self::Turn),
            (kind, len) => Err(W3dError::Channel { kind, len }),
        }
    }

    const fn len(self) -> usize {
        match self {
            Self::Move(_) => 1,
            Self::Turn => 4,
        }
    }
}

impl MotionKeys {
    /// The frames of `0..=frames` its keys fall on, rising, and `frames`, past the last, where a
    /// loop's last frame meets its first: a value between two of them is linear.
    pub(crate) fn key_frames(&self, frames: u32) -> Vec<u32> {
        match self {
            Self::Frames(_) => (0..=frames).collect(),
            Self::Timed(keys) => keys.frames.iter().copied().chain([frames]).collect(),
        }
    }

    /// Its move at `frame`, of `0..=frames`.
    pub(crate) fn move_at(&self, frame: u32, frames: u32) -> f32 {
        match self {
            Self::Frames(keys) => keys.value(frame, frames, [0.0])[0],
            Self::Timed(keys) => {
                let at = keys.at(frame);
                let Some(&next) = keys.frames.get(at + 1) else {
                    return keys.values[at];
                };
                // `WWMath::Lerp`, of the ratio `Get_Vector` takes.
                let ratio = (frame - keys.frames[at]) as f32 / (next - keys.frames[at]) as f32;
                keys.values[at] + (keys.values[at + 1] - keys.values[at]) * ratio
            }
        }
    }

    /// Its turn at `frame`, one of `key_frames(frames)`, so a value of its own and no blend of
    /// two.
    pub(crate) fn turn_at(&self, frame: u32, frames: u32) -> [f32; 4] {
        match self {
            Self::Frames(keys) => keys.value(frame, frames, [0.0, 0.0, 0.0, 1.0]),
            Self::Timed(keys) => {
                let at = keys.at(frame);
                debug_assert!(
                    keys.frames[at] == frame || at == keys.frames.len() - 1,
                    "a turn is read at a key or past the last"
                );
                keys.values[at * 4..at * 4 + 4]
                    .try_into()
                    .expect("a turn of four floats")
            }
        }
    }
}

impl FrameKeys {
    /// Its value at `frame`, of `L` floats: `frames`, past the last, that of frame 0, as
    /// `HRawAnimClass` interpolates the last frame toward the first; `identity` at a frame
    /// outside the channel's, as `MotionChannelClass::Get_Vector` gives.
    fn value<const L: usize>(&self, frame: u32, frames: u32, identity: [f32; L]) -> [f32; L] {
        let frame = if frame == frames { 0 } else { frame };
        let Some(at) = frame.checked_sub(self.first) else {
            return identity;
        };
        let at = usize::try_from(at).expect("a u32 fits usize") * L;
        self.values
            .get(at..at + L)
            .map_or(identity, |value| value.try_into().expect("L floats"))
    }
}

impl TimedKeys {
    /// The index of the key at or before `frame`, as `get_index` finds it; frame 0 is a key.
    fn at(&self, frame: u32) -> usize {
        self.frames.partition_point(|&key| key <= frame) - 1
    }
}

impl VisibilityChannel {
    /// Whether its pivot's meshes are shown at `frame`, as `Get_Bit` reads it.
    pub(crate) fn shown(&self, frame: u32) -> bool {
        let Some(bit) = frame
            .checked_sub(self.first)
            .filter(|&bit| bit < self.count)
        else {
            return self.default;
        };
        let bit = usize::try_from(bit).expect("a u32 fits usize");
        self.bits[bit / 8] & (1 << (bit % 8)) != 0
    }

    /// The frames of `0..frames` it hides its pivot's meshes in: runs, each from its first frame
    /// to before its end, in order, none touching the next.
    pub(crate) fn hidden_runs(&self, frames: u32) -> Vec<[u32; 2]> {
        let mut runs: Vec<[u32; 2]> = Vec::new();
        for frame in (0..frames).filter(|&frame| !self.shown(frame)) {
            match runs.last_mut() {
                Some(run) if run[1] == frame => run[1] += 1,
                _ => runs.push([frame, frame + 1]),
            }
        }
        runs
    }
}

/// The frames of a channel from `first` to `last`; an error for a last before the first.
fn channel_frames(first: u16, last: u16) -> Result<u32, W3dError> {
    let frames = u32::from(last).checked_sub(u32::from(first));
    Ok(frames.ok_or(W3dError::Counts)? + 1)
}

/// `count` floats, each finite.
fn finite_floats(fields: &mut W3dFields<'_>, count: usize) -> Result<Vec<f32>, W3dError> {
    let floats = (0..count)
        .map(|_| fields.cursor.f32())
        .collect::<Result<Vec<f32>, _>>()?;
    if !floats.iter().all(|float| float.is_finite()) {
        return Err(W3dError::NotFinite);
    }
    Ok(floats)
}

#[cfg(test)]
pub(crate) mod samples {
    use super::*;
    use crate::zero_hour::format::w3d::w3d_chunks::samples::chunk;
    use crate::zero_hour::format::w3d::w3d_fields::samples::name;

    fn floats(values: &[f32]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    /// A raw animation `animation` of `hierarchy`, its version 4.1, of `frames` at 30 a second, and
    /// `channels`, each of `channel` or `bit_channel`.
    pub(crate) fn raw(
        animation: &str,
        hierarchy: &str,
        frames: u32,
        channels: &[Vec<u8>],
    ) -> Vec<u8> {
        let header = [
            &0x0004_0001_u32.to_le_bytes()[..],
            &name(animation, NAME),
            &name(hierarchy, NAME),
            &frames.to_le_bytes(),
            &30_u32.to_le_bytes(),
        ]
        .concat();
        let body = [chunk(HEADER, false, &header), channels.concat()].concat();
        chunk(ANIMATION, true, &body)
    }

    /// A raw channel of `pivot`, of the type `flags`, from the frame `first`, a key a frame of
    /// `len` floats from `values`.
    pub(crate) fn channel(pivot: u16, flags: u16, len: u16, first: u16, values: &[f32]) -> Vec<u8> {
        let frames = u16::try_from(values.len()).unwrap() / len;
        let head = [first, first + frames - 1, len, flags, pivot, 0];
        let head: Vec<u8> = head.iter().flat_map(|field| field.to_le_bytes()).collect();
        chunk(CHANNEL, false, &[head, floats(values)].concat())
    }

    /// A visibility channel of `pivot` from the frame `first`, a bit a frame of `shown`, the
    /// frames outside them `default`.
    pub(crate) fn bit_channel(pivot: u16, first: u16, shown: &[bool], default: bool) -> Vec<u8> {
        let count = u16::try_from(shown.len()).unwrap();
        let head = [first, first + count - 1, VISIBILITY, pivot];
        let mut bytes: Vec<u8> = head.iter().flat_map(|field| field.to_le_bytes()).collect();
        bytes.push(u8::from(default));
        let mut bits = vec![0; shown.len().div_ceil(8)];
        for (at, _) in shown.iter().enumerate().filter(|(_, shown)| **shown) {
            bits[at / 8] |= 1 << (at % 8);
        }
        bytes.extend(bits);
        chunk(BIT_CHANNEL, false, &bytes)
    }

    /// A time-coded animation `animation` of `hierarchy`, of `frames` at 30 a second, of `flavor`, and
    /// `channels`, each of `timed`.
    pub(crate) fn compressed(
        animation: &str,
        hierarchy: &str,
        frames: u32,
        flavor: u16,
        channels: &[Vec<u8>],
    ) -> Vec<u8> {
        let header = [
            &1_u32.to_le_bytes()[..],
            &name(animation, NAME),
            &name(hierarchy, NAME),
            &frames.to_le_bytes(),
            &30_u16.to_le_bytes(),
            &flavor.to_le_bytes(),
        ]
        .concat();
        let body = [chunk(COMPRESSED_HEADER, false, &header), channels.concat()].concat();
        chunk(COMPRESSED_ANIMATION, true, &body)
    }

    /// A time-coded channel of `pivot`, of the type `flags`, of `keys`, each a time code and its
    /// `len` floats.
    pub(crate) fn timed(pivot: u16, flags: u8, len: u8, keys: &[(u32, &[f32])]) -> Vec<u8> {
        let mut bytes = [
            &u32::try_from(keys.len()).unwrap().to_le_bytes()[..],
            &pivot.to_le_bytes(),
            &[len, flags],
        ]
        .concat();
        for (code, values) in keys {
            bytes.extend(code.to_le_bytes());
            bytes.extend(floats(values));
        }
        chunk(COMPRESSED_CHANNEL, false, &bytes)
    }
}

#[cfg(test)]
mod tests;
