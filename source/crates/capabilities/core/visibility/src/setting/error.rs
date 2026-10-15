use campfire_state_types::{DataKind, Recipients};
use thiserror::Error;

use crate::setting::Setting;

/// Why a mode's `[visibility]`, or a host's setting, does not set a session's visibility (design
/// 14, D4, D14).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VisibilityError {
    /// A name that is no setting.
    #[error("{0:?} is no setting")]
    UnknownSetting(String),
    /// A kind whose audience no mode sets: vision decides it, or the owner alone holds it.
    #[error("no mode sets the audience of {}", .0.name())]
    NotSettable(DataKind),
    /// A value the setting does not take.
    #[error("{value:?} is no value of {setting}")]
    UnknownValue { setting: Setting, value: String },
    /// A recipient that the kind's audience holds, or lacks, whatever a mode says.
    #[error("no mode adds {recipient} to the audience of {}, or takes them from it", .kind.name())]
    Recipient {
        kind: DataKind,
        recipient: Recipients,
    },
    #[error("{0} is named twice")]
    Repeated(Setting),
    #[error("{recipient} is named twice for {}", .kind.name())]
    RepeatedRecipient {
        kind: DataKind,
        recipient: Recipients,
    },
    /// A host's setting that the mode's `host` list does not leave to the host.
    #[error("the mode does not leave {0} to the host")]
    NotLeft(Setting),
    /// A host's setting that is not of the form `name=value`.
    #[error("{0:?} is no setting of the form name=value")]
    NotHostSetting(String),
}
