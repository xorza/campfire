/// The form of a system that has one for each side of a match, chosen as the match installs:
/// the server's, which computes the whole state, or a predicting client's, which predicts its
/// player's units and leaves to the server what the server alone computes. `InstallCx::by_role`
/// adds the form of the match's role, so no tick asks which side it runs on.
pub trait RoleForm: Send + Sync + 'static {
    /// Whether the form is a predicting client's.
    const PREDICTS: bool;
}

/// The server's form of a system.
#[derive(Debug)]
pub struct ServerForm;

/// A predicting client's form of a system.
#[derive(Debug)]
pub struct ClientForm;

impl RoleForm for ServerForm {
    const PREDICTS: bool = false;
}

impl RoleForm for ClientForm {
    const PREDICTS: bool = true;
}
