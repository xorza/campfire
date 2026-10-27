use super::*;
use crate::zero_hour::format::wav::samples::wav;

#[test]
fn ima_adpcm_decodes_each_block_from_its_header_by_the_step_tables() {
    // A mono block: its header's sample 100 at step index 0, then the codes 7, 0, 8 and 0, low
    // four bits first. Code 7 at step 7: 7/8 + 7/4 + 7/2 + 7 = 0 + 1 + 3 + 7 = 11, so 111, and the
    // index moves 8 to step 16; code 0: 16/8 = 2, so 113, index 7, step 14; code 8: −(14/8) = −1,
    // so 112, index 6, step 13; code 0: 13/8 = 1, so 113.
    let mono = wav([17, 1, 6, 4], 22_050, None, &[100, 0, 0, 0, 0x07, 0x08]);
    let file = WavFile::read(&mono).unwrap();
    assert_eq!(
        (file.encoding, file.channels, file.rate),
        (WavEncoding::ImaAdpcm, 1, 22_050)
    );
    assert_eq!(file.decode(), [100, 111, 113, 112, 113]);
    // A `fact` chunk of 4 frames drops the last.
    let counted = wav([17, 1, 6, 4], 22_050, Some(4), &[100, 0, 0, 0, 0x07, 0x08]);
    assert_eq!(
        WavFile::read(&counted).unwrap().decode(),
        [100, 111, 113, 112]
    );

    // A stereo block: the left's header of 0 at index 0, the right's of −5 at index 88, then a
    // run of four bytes of each, the left's codes all 0, which stay at 0 and index 0; the right's
    // 0, 8, 0, 8, 1, 9, 2, 10, from the largest step, 32,767: 0 adds 32,767/8 = 4,095, so 4,090,
    // and so on as the step falls. The frames interleave the two.
    let header = [0, 0, 0, 0, 0xfb, 0xff, 88, 0];
    let codes = [0, 0, 0, 0, 0x80, 0x80, 0x91, 0xa2];
    let stereo = wav([17, 2, 16, 4], 44_100, None, &[header, codes].concat());
    let right = [4090, 366, 3751, 674, 9068, 1438, 13000, 2489];
    let mut frames = vec![0, -5];
    frames.extend(right.iter().flat_map(|&sample| [0, sample]));
    assert_eq!(WavFile::read(&stereo).unwrap().decode(), frames);
    // A sum past 16 bits holds at their end: 32,000 plus every step of code 7 at index 88.
    let loud = wav([17, 1, 5, 4], 22_050, None, &[0x00, 0x7d, 88, 0, 0x07]);
    assert_eq!(WavFile::read(&loud).unwrap().decode()[1], 32_767);
}

#[test]
fn a_pcm_file_writes_and_reads_back_and_another_encoding_is_refused() {
    let adpcm = wav([17, 2, 16, 4], 44_100, None, &[0; 16]);
    let file = WavFile::read(&adpcm).unwrap();
    let samples = [1, -2, 300, -32_768];
    let pcm = file.pcm_file(&samples);
    let read = WavFile::read(&pcm).unwrap();
    assert_eq!(
        (read.encoding, read.channels, read.rate),
        (WavEncoding::Pcm16, 2, 44_100)
    );
    let data: Vec<u8> = samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect();
    assert_eq!(read.data, data);
    // The 44 bytes of a canonical header, then the samples.
    assert_eq!(pcm.len(), 44 + 8);

    // No RIFF, an MP3 in a WAV, 8-bit PCM, no data, no channel, and a stereo block of a part of
    // a run, each refused.
    assert_eq!(WavFile::read(b"RIFX").unwrap_err(), WavError::NotRiff);
    let refused =
        |format, data: &[u8]| WavFile::read(&wav(format, 22_050, None, data)).unwrap_err();
    assert_eq!(
        refused([85, 1, 1, 0], &[]),
        WavError::Encoding { tag: 85, bits: 0 }
    );
    assert_eq!(
        refused([1, 1, 1, 8], &[]),
        WavError::Encoding { tag: 1, bits: 8 }
    );
    assert_eq!(refused([17, 0, 8, 4], &[]), WavError::Layout);
    assert_eq!(refused([17, 2, 12, 4], &[]), WavError::Layout);
    let mut no_data = wav([1, 1, 2, 16], 22_050, None, &[]);
    no_data.truncate(no_data.len() - 8);
    assert_eq!(WavFile::read(&no_data).unwrap_err(), WavError::NoData);
}
