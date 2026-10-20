use std::sync::Mutex;

use campfire_protocol::Signer;

/// The signer of a session's key, which `SimClientPlugin` hands to its app once, as its build runs,
/// so no plugin keeps the key for the app's life: a plugin's build takes it by reference, and could
/// otherwise only copy it out.
#[derive(Debug)]
pub struct SessionSigner(Mutex<Option<Signer>>);

impl SessionSigner {
    pub const fn new(signer: Signer) -> Self {
        Self(Mutex::new(Some(signer)))
    }

    /// The signer, which a plugin's build takes once.
    pub(crate) fn take(&self) -> Signer {
        self.0
            .lock()
            .expect("no thread panics holding the session's signer")
            .take()
            .expect("a client's plugin builds once")
    }
}
