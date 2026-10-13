use bevy_ecs::resource::Resource;

/// Marks a world whose state a restore is putting in, or a test is making flawed for a restore to
/// refuse: its values are untrusted until the restore checks pass, so a hook that asserts a rule
/// of the state leaves the rule to those checks, which refuse a flaw with an error.
#[derive(Resource, Debug)]
pub struct Restoring;
