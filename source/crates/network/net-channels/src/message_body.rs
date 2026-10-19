use crate::fragment_body::FragmentBody;
use crate::message_id::MessageId;
use crate::single_body::SingleBody;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MessageBody {
    Single(SingleBody),
    Fragment(FragmentBody),
}

impl MessageBody {
    pub(crate) const fn message_id(&self) -> Option<MessageId> {
        match self {
            Self::Single(data) => data.id,
            Self::Fragment(data) => Some(data.message_id),
        }
    }
}

impl From<FragmentBody> for MessageBody {
    fn from(value: FragmentBody) -> Self {
        Self::Fragment(value)
    }
}

impl From<SingleBody> for MessageBody {
    fn from(value: SingleBody) -> Self {
        Self::Single(value)
    }
}
