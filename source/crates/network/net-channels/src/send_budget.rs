use crate::channel_setup::DEFAULT_MAX_MESSAGE_LEN;
use crate::receive_budget::DEFAULT_RECEIVE_BUDGET;

/// The bytes a link's channels queue at most: its staged messages, and what its senders hold until
/// they send them, or on a reliable channel until they are acknowledged. Twice the default
/// `max_message_len`, as the receive budget is (11-net-stack/transport.md, A link's queued bytes
/// have a budget).
pub(crate) const SEND_BUDGET: usize = 2 * DEFAULT_MAX_MESSAGE_LEN;

// A message the receive budget holds, as each channel's must be, an empty link queues.
const _: () = assert!(DEFAULT_RECEIVE_BUDGET <= SEND_BUDGET);
