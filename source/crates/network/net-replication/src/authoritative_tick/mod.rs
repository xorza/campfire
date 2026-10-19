//! The server's tick of each Replicon message, in the message's userdata: Replicon's own tick
//! counts its sends and orders its messages, while prediction, rollback and the timelines run on
//! the server's `NetTick`, so a client resolves each Replicon tick through the
//! [`ConfirmedTickMap`](crate::ConfirmedTickMap) before it uses it.

use crate::confirmed_tick_error::ConfirmedTickError;
use campfire_net_core::NetTick;

pub(crate) const AUTHORITATIVE_TICK_LEN: usize = size_of::<NetTick>();

pub(crate) const fn encode_authoritative_tick(
    authoritative_tick: NetTick,
) -> [u8; AUTHORITATIVE_TICK_LEN] {
    authoritative_tick.0.to_le_bytes()
}

pub(crate) const fn decode_authoritative_tick(bytes: &[u8]) -> Result<NetTick, ConfirmedTickError> {
    if bytes.len() != AUTHORITATIVE_TICK_LEN {
        return Err(ConfirmedTickError::InvalidUserdataLength(bytes.len()));
    }
    Ok(NetTick(u32::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3],
    ])))
}

#[cfg(test)]
mod tests;
