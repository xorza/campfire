/// The name a payload's sender gives it, which the session reports the payload's fate by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PayloadToken(pub u64);
