use bevy_reflect::Reflect;

#[derive(Debug, Clone, Copy, Default, Reflect)]
pub enum RollbackMode {
    /// We always rollback on newly received confirmed state, without comparing it with the
    /// predicted history.
    ///
    /// It can be useful to always do rollbacks to test that your game can handle the CPU demand of
    /// doing the frequent rollbacks.
    Always,
    #[default]
    /// We check if we should rollback by comparing with a previous value.
    Check,
    /// We don't rollback or do any checks.
    Disabled,
}
