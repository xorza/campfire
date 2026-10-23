/// A type the network carries: every `Send + Sync + 'static` type is one, and its registration
/// gives its encoding.
pub trait Message: Send + Sync + 'static {}

impl<T: Send + Sync + 'static> Message for T {}
