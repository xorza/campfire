use campfire_net_bytes::wrapping_id;

// Internal id that we assign to each message sent over the network
wrapping_id!(MessageId);

#[cfg(test)]
mod tests;
