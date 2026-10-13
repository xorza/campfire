use thiserror::Error;

/// Why a unit type's AI does not load. Packages are untrusted, so each is an expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum AiError {
    /// The think period is too long to count in ticks.
    #[error("think period too long to count in ticks")]
    TimeTooLarge,
    /// The script has no `on_think(ctx, unit)`.
    #[error("AI script has no on_think(ctx, unit)")]
    NoThink,
}
