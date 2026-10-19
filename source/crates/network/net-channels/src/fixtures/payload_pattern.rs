#[derive(Clone, Copy, Debug)]
pub enum PayloadPattern {
    Repeated,
    Structured,
    RandomAscii,
}

impl PayloadPattern {
    /// A deterministic `len`-byte message for the `message_index`-th send of a case.
    pub fn payload(self, len: usize, message_index: usize) -> String {
        match self {
            Self::Repeated => "A".repeat(len),
            Self::Structured => structured_payload(len, message_index),
            Self::RandomAscii => random_ascii_payload(len, message_index),
        }
    }
}

fn structured_payload(len: usize, message_index: usize) -> String {
    let seed = format!(
        "{{entity:{:04},x:{:04},y:{:04},state:idle,tag:replicated}};",
        message_index,
        message_index % 128,
        (message_index * 3) % 128
    );
    repeat_to_len(&seed, len)
}

fn random_ascii_payload(len: usize, message_index: usize) -> String {
    let mut state =
        0x9e37_79b9_7f4a_7c15u64 ^ u64::try_from(message_index).expect("a usize fits a u64");
    let mut payload = String::with_capacity(len);
    for _ in 0..len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let byte = b'!' + u8::try_from(state % 94).expect("a remainder of 94 fits a u8");
        payload.push(char::from(byte));
    }
    payload
}

fn repeat_to_len(seed: &str, len: usize) -> String {
    let mut payload = String::with_capacity(len);
    while payload.len() + seed.len() <= len {
        payload.push_str(seed);
    }
    if payload.len() < len {
        payload.push_str(&seed[..len - payload.len()]);
    }
    payload
}
