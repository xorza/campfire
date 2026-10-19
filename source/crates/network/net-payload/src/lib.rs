//! The seam between the frame and the packet session of each link: the queues they hand each
//! other, the payloads in them, the numbers and fates of their packets, and the codes and
//! reasons a link ends with (13-net-stack/session.md, The seam is a set of queues on `Link`).

mod app_code;
mod mtu;
mod packet_number;
mod payload_fate;
mod payload_kind;
mod payload_token;
mod receive_payload;
mod session_queues;
mod unlink_reason;

pub use crate::app_code::AppCode;
pub use crate::mtu::DEFAULT_MTU;
pub use crate::packet_number::PacketNumber;
pub use crate::payload_fate::PayloadFate;
pub use crate::payload_kind::PayloadKind;
pub use crate::payload_token::PayloadToken;
pub use crate::receive_payload::{ReceivePayload, SendPayload};
pub use crate::session_queues::{OpenedPayload, OutgoingPayload, SessionQueues};
pub use crate::unlink_reason::UnlinkReason;
