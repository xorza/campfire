use crate::fragment_data::FragmentData;
use crate::message_id::MessageId;
use crate::single_data::SingleData;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MessageData {
    Single(SingleData),
    Fragment(FragmentData),
}

impl MessageData {
    pub(crate) const fn message_id(&self) -> Option<MessageId> {
        match self {
            Self::Single(data) => data.id,
            Self::Fragment(data) => Some(data.message_id),
        }
    }
}

impl From<FragmentData> for MessageData {
    fn from(value: FragmentData) -> Self {
        Self::Fragment(value)
    }
}

impl From<SingleData> for MessageData {
    fn from(value: SingleData) -> Self {
        Self::Single(value)
    }
}
