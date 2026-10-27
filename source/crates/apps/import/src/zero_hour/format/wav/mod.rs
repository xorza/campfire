//! The original's WAV files, read as RIFF chunks, and IMA ADPCM decoded to 16-bit PCM
//! (09-zero-hour.md, A sound is PCM, the music the game's MP3).

use crate::zero_hour::format::byte_cursor::ByteCursor;
use crate::zero_hour::format::error::WavError;

/// A WAV file of the original: its encoding, its channels, its samples a second, the bytes of one
/// block of its data, its data, and the frames its `fact` chunk counts, none for a file of none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WavFile<'a> {
    pub(crate) encoding: WavEncoding,
    pub(crate) channels: u16,
    pub(crate) rate: u32,
    block_align: u16,
    data: &'a [u8],
    frames: Option<u32>,
}

/// A WAV file's encoding: PCM of 16 bits, or IMA ADPCM of 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WavEncoding {
    Pcm16,
    ImaAdpcm,
}

/// The IMA ADPCM step sizes, by the step's index.
const STEPS: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];

/// How each code's magnitude moves the step's index.
const INDEX_MOVES: [i32; 8] = [-1, -1, -1, -1, 2, 4, 6, 8];

/// The bytes of a block's header for each channel: its first sample, its step's index, and a
/// byte of nothing.
const HEADER: usize = 4;

/// The bytes of codes a channel takes in turn in a block of several channels, eight codes.
const RUN: usize = 4;

/// The format tags of PCM and of IMA ADPCM.
const TAG_PCM: u16 = 1;
const TAG_IMA_ADPCM: u16 = 17;

impl<'a> WavFile<'a> {
    /// The WAV file `bytes`; an error for one of no RIFF `WAVE` header, of no `fmt ` or `data`
    /// chunk, of another encoding than PCM of 16 bits or IMA ADPCM of 4, of no channel, or an
    /// ADPCM block too short for its headers.
    pub(crate) fn read(bytes: &'a [u8]) -> Result<Self, WavError> {
        let mut file = ByteCursor::new(bytes);
        let riff = file.take(12).map_err(|_| WavError::NotRiff)?;
        if &riff[..4] != b"RIFF" || &riff[8..] != b"WAVE" {
            return Err(WavError::NotRiff);
        }
        let mut format = None;
        let mut data = None;
        let mut frames = None;
        while !file.is_empty() {
            let id = file.take(4).map_err(|_| WavError::CutShort)?;
            let size = file.u32().map_err(|_| WavError::CutShort)?;
            let size = usize::try_from(size).expect("a u32 fits usize");
            // The last chunk may end short of its size, as a file the game plays does.
            let body = file
                .take(size.min(file.rest().len()))
                .expect("within the rest");
            if size % 2 == 1 {
                file.take(1).ok();
            }
            match id {
                b"fmt " => format = Some(body),
                b"data" => data = Some(body),
                b"fact" => {
                    let count = ByteCursor::new(body)
                        .u32()
                        .map_err(|_| WavError::CutShort)?;
                    frames = Some(count);
                }
                _ => {}
            }
        }
        let mut format = ByteCursor::new(format.ok_or(WavError::NoFormat)?);
        let fields = (|| {
            Some((
                format.u16().ok()?,
                format.u16().ok()?,
                format.u32().ok()?,
                format.u32().ok()?,
                format.u16().ok()?,
                format.u16().ok()?,
            ))
        })();
        let (tag, channels, rate, _, block_align, bits) = fields.ok_or(WavError::NoFormat)?;
        let encoding = match (tag, bits) {
            (TAG_PCM, 16) => WavEncoding::Pcm16,
            (TAG_IMA_ADPCM, 4) => WavEncoding::ImaAdpcm,
            _ => return Err(WavError::Encoding { tag, bits }),
        };
        let header = HEADER * usize::from(channels);
        let runs = RUN * usize::from(channels);
        let block = usize::from(block_align);
        let fits = channels > 0
            && (encoding == WavEncoding::Pcm16
                || (block > header && (channels == 1 || (block - header).is_multiple_of(runs))));
        if !fits {
            return Err(WavError::Layout);
        }
        Ok(Self {
            encoding,
            channels,
            rate,
            block_align,
            data: data.ok_or(WavError::NoData)?,
            frames,
        })
    }

    /// The samples of an IMA ADPCM file, frame by frame, each channel's in order, as each block
    /// decodes: its header's sample, then its codes, a channel's eight codes of each four bytes
    /// in turn, a byte's low four bits first; a last block cut short decodes its whole runs, a
    /// mono one each of its bytes, and the samples stop at the frames its `fact` chunk counts.
    pub(crate) fn decode(&self) -> Vec<i16> {
        debug_assert_eq!(self.encoding, WavEncoding::ImaAdpcm, "a file of codes");
        let channels = usize::from(self.channels);
        let mut samples = Vec::new();
        let mut frames = vec![0_i16; 8 * channels];
        for block in self.data.chunks(usize::from(self.block_align)) {
            let Some((header, codes)) = block.split_at_checked(HEADER * channels) else {
                break;
            };
            let mut states: Vec<Decoder> = header
                .as_chunks::<HEADER>()
                .0
                .iter()
                .map(|header| Decoder {
                    sample: i32::from(i16::from_le_bytes([header[0], header[1]])),
                    index: i32::from(header[2]).min(88),
                })
                .collect();
            samples.extend(states.iter().map(|state| state.output()));
            let mut runs = codes.chunks_exact(RUN * channels);
            for run in &mut runs {
                for (channel, bytes) in run.as_chunks::<RUN>().0.iter().enumerate() {
                    let state = &mut states[channel];
                    for (at, &byte) in bytes.iter().enumerate() {
                        for (half, code) in [byte & 0x0f, byte >> 4].into_iter().enumerate() {
                            let sample = state.next(code);
                            frames[(at * 2 + half) * channels + channel] = sample;
                        }
                    }
                }
                samples.extend_from_slice(&frames);
            }
            // A mono block's last codes need no whole run.
            if channels == 1 {
                for &byte in runs.remainder() {
                    samples.extend([byte & 0x0f, byte >> 4].map(|code| states[0].next(code)));
                }
            }
        }
        if let Some(frames) = self.frames {
            let frames = usize::try_from(frames).expect("a u32 fits usize");
            samples.truncate(frames * channels);
        }
        samples
    }

    /// The WAV file of 16-bit PCM of `samples`, of its channels and its rate.
    pub(crate) fn pcm_file(&self, samples: &[i16]) -> Vec<u8> {
        let data = u32::try_from(samples.len() * 2).expect("a WAV's data fits u32");
        let channels = self.channels;
        let block = channels * 2;
        let mut file = Vec::with_capacity(44 + samples.len() * 2);
        file.extend_from_slice(b"RIFF");
        file.extend_from_slice(&(36 + data).to_le_bytes());
        file.extend_from_slice(b"WAVEfmt ");
        file.extend_from_slice(&16_u32.to_le_bytes());
        file.extend_from_slice(&TAG_PCM.to_le_bytes());
        file.extend_from_slice(&channels.to_le_bytes());
        file.extend_from_slice(&self.rate.to_le_bytes());
        file.extend_from_slice(&(self.rate * u32::from(block)).to_le_bytes());
        file.extend_from_slice(&block.to_le_bytes());
        file.extend_from_slice(&16_u16.to_le_bytes());
        file.extend_from_slice(b"data");
        file.extend_from_slice(&data.to_le_bytes());
        for sample in samples {
            file.extend_from_slice(&sample.to_le_bytes());
        }
        file
    }
}

/// A channel's state as its codes decode: its last sample and its step's index.
#[derive(Debug, Clone, Copy)]
struct Decoder {
    sample: i32,
    index: i32,
}

impl Decoder {
    /// Its sample as 16 bits.
    fn output(self) -> i16 {
        i16::try_from(self.sample).expect("a sample held within 16 bits")
    }

    /// The sample of `code`, four bits: the step's eighth, plus its quarter, half and whole by
    /// the code's three low bits, added or, by its fourth, taken away, held within 16 bits; the
    /// step's index then moves by the code's magnitude, held within the table.
    fn next(&mut self, code: u8) -> i16 {
        let step = STEPS[usize::try_from(self.index).expect("an index within the table")];
        let mut difference = step >> 3;
        if code & 1 != 0 {
            difference += step >> 2;
        }
        if code & 2 != 0 {
            difference += step >> 1;
        }
        if code & 4 != 0 {
            difference += step;
        }
        if code & 8 != 0 {
            difference = -difference;
        }
        self.sample = (self.sample + difference).clamp(i32::from(i16::MIN), i32::from(i16::MAX));
        self.index = (self.index + INDEX_MOVES[usize::from(code & 7)]).clamp(0, 88);
        self.output()
    }
}

#[cfg(test)]
pub(crate) mod samples {
    /// A WAV file of the tag, channels, rate, block and bits given, its `fact` chunk of `frames` when
    /// it has one, and its data.
    pub(crate) fn wav(format: [u16; 4], rate: u32, frames: Option<u32>, data: &[u8]) -> Vec<u8> {
        let [tag, channels, block, bits] = format;
        let mut chunks = Vec::new();
        chunks.extend_from_slice(b"fmt ");
        chunks.extend_from_slice(&16_u32.to_le_bytes());
        for field in [tag, channels] {
            chunks.extend_from_slice(&field.to_le_bytes());
        }
        chunks.extend_from_slice(&rate.to_le_bytes());
        chunks.extend_from_slice(&0_u32.to_le_bytes());
        for field in [block, bits] {
            chunks.extend_from_slice(&field.to_le_bytes());
        }
        if let Some(frames) = frames {
            chunks.extend_from_slice(b"fact");
            chunks.extend_from_slice(&4_u32.to_le_bytes());
            chunks.extend_from_slice(&frames.to_le_bytes());
        }
        chunks.extend_from_slice(b"data");
        chunks.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
        chunks.extend_from_slice(data);
        let mut file = b"RIFF".to_vec();
        file.extend_from_slice(&u32::try_from(chunks.len() + 4).unwrap().to_le_bytes());
        file.extend_from_slice(b"WAVE");
        file.extend_from_slice(&chunks);
        file
    }
}

#[cfg(test)]
mod tests;
