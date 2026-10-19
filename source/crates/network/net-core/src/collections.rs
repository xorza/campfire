use bevy_platform::collections;
use bevy_platform::hash::FixedHasher;

// A default type parameter does not drive inference, so `HashMap::default()` on Bevy's map leaves
// the hasher unknown: this alias fixes it to `FixedHasher`.
pub type HashMap<K, V> = collections::HashMap<K, V, FixedHasher>;
