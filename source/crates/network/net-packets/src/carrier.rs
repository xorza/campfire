use core::fmt::Debug;

/// What carries a session for an endpoint's host, as an entity does in an ECS: the core names it
/// in its news, and knows nothing more of it.
pub trait Carrier: Copy + Eq + Debug + Send + 'static {}

impl<T: Copy + Eq + Debug + Send + 'static> Carrier for T {}
