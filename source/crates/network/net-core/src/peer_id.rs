#[cfg(feature = "reflect")]
use bevy_reflect::Reflect;
use campfire_net_bytes::SerializationError;
use campfire_net_bytes::ToBytes;
use campfire_net_bytes::WriteInteger;
use campfire_net_bytes::{ReadInteger, Reader};
use core::fmt::{Display, Formatter};
use core::net::SocketAddr;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "reflect", derive(Reflect))]
pub enum PeerId {
    Entity(u64),
    /// A peer that a link names by its address.
    #[cfg_attr(feature = "reflect", reflect(ignore))]
    Address(SocketAddr),
    /// A local client to use when running in HostServer mode
    Local(u64),
    /// Refers to the server
    Server,
}

impl ToBytes for PeerId {
    fn bytes_len(&self) -> usize {
        match self {
            Self::Entity(_) => 1 + 8,
            Self::Address(address) => {
                // 1 byte for variant + address bytes + 2 bytes for port
                match address {
                    SocketAddr::V4(_) => 1 + 4 + 2,
                    SocketAddr::V6(_) => 1 + 16 + 2,
                }
            }
            Self::Local(_) => 1 + 8,
            Self::Server => 1,
        }
    }

    fn to_bytes(&self, buffer: &mut impl WriteInteger) {
        match self {
            Self::Entity(id) => {
                buffer.write_u8(0);
                buffer.write_u64(*id);
            }
            Self::Address(address) => match address {
                SocketAddr::V4(v4) => {
                    buffer.write_u8(1);
                    for byte in &v4.ip().octets() {
                        buffer.write_u8(*byte);
                    }
                    buffer.write_u16(v4.port());
                }
                SocketAddr::V6(v6) => {
                    buffer.write_u8(2);
                    for segment in &v6.ip().segments() {
                        buffer.write_u16(*segment);
                    }
                    buffer.write_u16(v6.port());
                }
            },
            Self::Local(id) => {
                buffer.write_u8(5);
                buffer.write_u64(*id);
            }
            Self::Server => {
                buffer.write_u8(6);
            }
        }
    }

    fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError>
    where
        Self: Sized,
    {
        match buffer.read_u8()? {
            0 => Ok(Self::Entity(buffer.read_u64()?)),
            1 => {
                let a = buffer.read_u8()?;
                let b = buffer.read_u8()?;
                let c = buffer.read_u8()?;
                let d = buffer.read_u8()?;
                let port = buffer.read_u16()?;
                let address = SocketAddr::new(
                    core::net::IpAddr::V4(core::net::Ipv4Addr::new(a, b, c, d)),
                    port,
                );
                Ok(Self::Address(address))
            }
            2 => {
                let mut segments = [0u16; 8];
                for segment in &mut segments {
                    *segment = buffer.read_u16()?;
                }
                let port = buffer.read_u16()?;
                let address = SocketAddr::new(
                    core::net::IpAddr::V6(core::net::Ipv6Addr::from(segments)),
                    port,
                );
                Ok(Self::Address(address))
            }
            5 => Ok(Self::Local(buffer.read_u64()?)),
            6 => Ok(Self::Server),
            _ => Err(SerializationError::InvalidValue),
        }
    }
}

impl PeerId {
    pub const fn is_local(&self) -> bool {
        matches!(self, Self::Local(_))
    }
}

impl Display for PeerId {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(self, f)
    }
}
