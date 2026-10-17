use bevy_ecs::resource::Resource;

/// What visits the resources a match's build puts in itself, before any module installs, as the
/// sim's world: the release's declarations read them through it, so the audit of a match's
/// resources knows each without a list of its own.
pub trait OwnResources {
    /// `R`, a resource the build puts in itself, which the install leaves as it is.
    fn own<R: Resource>(&mut self);
}
