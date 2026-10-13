use quinn_proto::VarInt;
use serde::{Deserialize, Serialize};

/// Why an end closed a link, which QUIC carries as the application's error code (design 15, D10).
/// The codes below `AppCode::FIRST` are `quic`'s own; an app names its own from there up, and
/// reads a peer's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AppCode(u64);

impl AppCode {
    /// The link ended with no reason given, as a removed link entity ends it.
    pub const ENDED: AppCode = AppCode(0);
    /// The endpoint dropped, as its app exited.
    pub const STOPPED: AppCode = AppCode(1);
    /// The connection's datagrams cannot carry a link's minimum MTU.
    pub const UNFIT: AppCode = AppCode(2);
    /// The first code an app may name.
    pub const FIRST: u32 = 16;

    /// The app's code `code`, `FIRST` or more.
    pub const fn new(code: u32) -> AppCode {
        assert!(
            code >= AppCode::FIRST,
            "the codes below FIRST are quic's own"
        );
        AppCode(code as u64)
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub(crate) const fn of(code: VarInt) -> AppCode {
        AppCode(code.into_inner())
    }

    pub(crate) fn varint(self) -> VarInt {
        VarInt::from_u64(self.0).expect("a code of quic's or an app's u32 fits a varint")
    }
}
