use crate::zero_hour::error::RefPackError;

/// EA's `RefPack`: an LZ77 stream of literal runs and copies of earlier bytes, as Zero Hour packs
/// its maps (`REF_decode` in the released source). Its header is two bytes of flags, big-endian,
/// then its decoded length in three bytes, or four with the flag `0x8000`, past an ignored length
/// of as many bytes with the flag `0x0100`. Each command is one of four forms by its first byte's
/// high bits, each taking literal bytes then a copy, and a last form that ends the stream.
#[derive(Debug)]
pub(crate) struct RefPack;

impl RefPack {
    /// The bytes `stream` decodes to, refused when it ends inside a command, copies from before
    /// its start, or writes other than its header's length.
    pub(crate) fn decode(stream: &[u8]) -> Result<Vec<u8>, RefPackError> {
        let mut reader = Reader { stream, at: 0 };
        let flags = u16::from_be_bytes([reader.byte()?, reader.byte()?]);
        let width = if flags & 0x8000 != 0 { 4 } else { 3 };
        if flags & 0x0100 != 0 {
            reader.take(width)?;
        }
        let len = reader
            .take(width)?
            .iter()
            .fold(0_usize, |len, &byte| len << 8 | usize::from(byte));
        // The header's length is untrusted, so the output grows as it is written, and no header
        // reserves more than its stream writes.
        let mut out = Vec::new();
        loop {
            let first = usize::from(reader.byte()?);
            let (literals, distance, count) = if first & 0x80 == 0 {
                let second = usize::from(reader.byte()?);
                (
                    first & 3,
                    ((first & 0x60) << 3) + second + 1,
                    ((first & 0x1C) >> 2) + 3,
                )
            } else if first & 0x40 == 0 {
                let [second, third] = [reader.byte()?, reader.byte()?].map(usize::from);
                (
                    second >> 6,
                    ((second & 0x3F) << 8) + third + 1,
                    (first & 0x3F) + 4,
                )
            } else if first & 0x20 == 0 {
                let [second, third, fourth] =
                    [reader.byte()?, reader.byte()?, reader.byte()?].map(usize::from);
                (
                    first & 3,
                    ((first & 0x10) << 12) + (second << 8) + third + 1,
                    ((first & 0x0C) << 6) + fourth + 5,
                )
            } else {
                let run = ((first & 0x1F) << 2) + 4;
                if run <= 112 {
                    (run, 0, 0)
                } else {
                    out.extend_from_slice(reader.take(first & 3)?);
                    break;
                }
            };
            out.extend_from_slice(reader.take(literals)?);
            let start = out
                .len()
                .checked_sub(distance)
                .ok_or(RefPackError::BeforeStart)?;
            // A copy may overlap what it writes, as a run of one byte repeats it.
            for at in start..start + count {
                out.push(out[at]);
            }
            if out.len() > len {
                return Err(RefPackError::TooLong(len));
            }
        }
        if out.len() != len {
            return Err(if out.len() > len {
                RefPackError::TooLong(len)
            } else {
                RefPackError::TooShort {
                    written: out.len(),
                    len,
                }
            });
        }
        Ok(out)
    }
}

/// A stream's bytes and how far a decode has read them.
#[derive(Debug)]
struct Reader<'a> {
    stream: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn byte(&mut self) -> Result<u8, RefPackError> {
        Ok(self.take(1)?[0])
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], RefPackError> {
        let bytes = self
            .stream
            .get(self.at..self.at + count)
            .ok_or(RefPackError::Short)?;
        self.at += count;
        Ok(bytes)
    }
}

#[cfg(test)]
pub(crate) mod internals {
    /// `bytes` as a `RefPack` stream of literal runs alone: its header of 3 length bytes, runs
    /// of up to 112 bytes in fours, and the end with the last 0 to 3.
    pub(crate) fn literal(bytes: &[u8]) -> Vec<u8> {
        let len = u32::try_from(bytes.len()).unwrap().to_be_bytes();
        assert_eq!(len[0], 0, "a length of 3 bytes");
        let mut stream = [&[0x10, 0xFB][..], &len[1..]].concat();
        let (runs, rest) = bytes.split_at(bytes.len() / 4 * 4);
        for run in runs.chunks(112) {
            stream.push(0xE0 | u8::try_from((run.len() - 4) / 4).unwrap());
            stream.extend(run);
        }
        stream.push(0xFC | u8::try_from(rest.len()).unwrap());
        stream.extend(rest);
        stream
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_command_form_decodes_to_the_bytes_it_names() {
        // Flags 0x10FB, a length of 3 bytes, 19. Then: a literal run of 4 (0xE0, `(0 << 2) +
        // 4`); a short copy of 4 from 4 back (0x04: count `(1) + 3`; 3: distance `3 + 1`); an
        // int copy of 4 from 8 back (0x80: count `0 + 4`; 0x00 0x07: distance `7 + 1`); a very
        // int copy of 5 from 12 back (0xC0: count `0 + 0 + 5`; 0x00 0x0B: distance `11 + 1`; 0x00);
        // and the end with 2 literal bytes (0xFE).
        let stream = [
            &[0x10, 0xFB, 0x00, 0x00, 19][..],
            &[0xE0],
            b"abcd",
            &[0x04, 3],
            &[0x80, 0x00, 0x07],
            &[0xC0, 0x00, 0x0B, 0x00],
            &[0xFE],
            b"XY",
        ]
        .concat();
        assert_eq!(RefPack::decode(&stream).unwrap(), b"abcdabcdabcdabcdaXY");

        // A short copy that takes a literal first and overlaps what it writes: `z`, then 3 of it
        // from 1 back, then the end with `q`. With the flags 0x8000 and 0x0100, the length is 4
        // bytes, past 4 ignored ones.
        let body = [&[0x01, 0x00][..], b"z", &[0xFD], b"q"].concat();
        let short = [&[0x10, 0xFB, 0, 0, 5][..], &body].concat();
        assert_eq!(RefPack::decode(&short).unwrap(), b"zzzzq");
        let long = [&[0x91, 0xFB, 0, 0, 0, 9, 0, 0, 0, 5][..], &body].concat();
        assert_eq!(RefPack::decode(&long).unwrap(), b"zzzzq");

        // Cut short, a copy before the start, and a length the stream does not write.
        assert_eq!(
            RefPack::decode(&short[..short.len() - 1]),
            Err(RefPackError::Short)
        );
        assert_eq!(RefPack::decode(&[0x10, 0xFB]), Err(RefPackError::Short));
        assert_eq!(
            RefPack::decode(&[0x10, 0xFB, 0, 0, 3, 0x04, 0x05]),
            Err(RefPackError::BeforeStart)
        );
        let with_len = |len| [&[0x10, 0xFB, 0, 0, len][..], &body].concat();
        assert_eq!(RefPack::decode(&with_len(4)), Err(RefPackError::TooLong(4)));
        assert_eq!(
            RefPack::decode(&with_len(9)),
            Err(RefPackError::TooShort { written: 5, len: 9 })
        );
    }
}
