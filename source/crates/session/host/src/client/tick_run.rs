use bevy_ecs::resource::Resource;

/// How the client runs the tick it runs now: one it predicts the first time, or one a rollback
/// replays, which it predicted before, so what plays once at a tick, as a shot's effect, plays in
/// the first alone (02-engine-core.md, A shot plays in its tick's first prediction).
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TickRun {
    #[default]
    First,
    Replay,
}
