/// A [`Message`] is basically any type that can be (de)serialized over the network.
///
/// Every type that can be sent over the network must implement this trait.
///
pub trait Message: Send + Sync + 'static {}

impl<T: Send + Sync + 'static> Message for T {}
