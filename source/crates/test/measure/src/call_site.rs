/// Where an allocation's call stack passes: a function a dependency compiled whole keeps its
/// whole name, as `bevy_replicon::client::receive_replication`, while one inlined into the
/// workspace's code keeps only its short name and its source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallSite {
    /// A part of the function's name.
    Function(&'static str),
    /// The end of its source file's path, in whole components, as
    /// `bevy_ecs-0.20.0/src/bundle/remove.rs`.
    File(&'static str),
}
