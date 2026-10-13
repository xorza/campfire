use bevy_ecs::resource::Resource;

/// A resource a tick fills and empties: no state, and clear between the ticks of a running match,
/// so a restore needs none of it; a match that ended runs no stage, which would read it. The
/// installer puts it in at its default; the resource audit fails a match that ends a tick with one
/// not clear.
pub trait Scratch: Resource + Default {
    /// Whether it holds nothing of a tick.
    fn is_clear(&self) -> bool;
}
