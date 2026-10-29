use core::fmt::{self, Debug, Formatter};

use bevy_app::App;

/// What a registry's registrations add to the app once it finishes its plugins, in their order,
/// as a registration's systems need what later plugins add.
#[derive(Default)]
pub struct PendingInstalls(Vec<Box<dyn FnOnce(&mut App) + Send + Sync>>);

/// How many it holds, as an install has no name.
impl Debug for PendingInstalls {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("PendingInstalls")
            .field("len", &self.0.len())
            .finish()
    }
}

impl PendingInstalls {
    /// Queues `install` after those queued before.
    pub fn push(&mut self, install: impl FnOnce(&mut App) + Send + Sync + 'static) {
        self.0.push(Box::new(install));
    }

    /// Takes every install queued, which the registry runs into `app`.
    #[must_use]
    pub fn take(&mut self) -> Self {
        Self(core::mem::take(&mut self.0))
    }

    /// Runs each install into `app`, in their order.
    pub fn run(self, app: &mut App) {
        for install in self.0 {
            install(app);
        }
    }
}
