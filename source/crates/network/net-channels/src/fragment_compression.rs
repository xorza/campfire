use campfire_net_bytes::Reader;
use campfire_net_bytes::SerializationError;
use campfire_net_bytes::ToBytes;
use campfire_net_bytes::WriteInteger;
use core::fmt::Debug;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum FragmentCompression {
    #[default]
    None,
    Lz4,
}

impl FragmentCompression {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Lz4 => "lz4",
        }
    }
}

impl ToBytes for FragmentCompression {
    fn bytes_len(&self) -> usize {
        1
    }

    fn to_bytes(&self, buffer: &mut impl WriteInteger) {
        match self {
            Self::None => 0u8.to_bytes(buffer),
            Self::Lz4 => 1u8.to_bytes(buffer),
        }
    }

    fn from_bytes(buffer: &mut Reader) -> Result<Self, SerializationError>
    where
        Self: Sized,
    {
        match u8::from_bytes(buffer)? {
            0 => Ok(Self::None),
            1 => Ok(Self::Lz4),
            _ => Err(SerializationError::InvalidValue),
        }
    }
}
