#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::sync::Arc;

    use bevy_ecs::world::World;

    use crate::client::client_dir::ClientDir;
    use crate::client::receipt_writer::ReceiptWriter;

    /// What a client's world gives the tests and benches of any crate.
    pub trait SimClientWorldInternals {
        /// Keeps the client's receipts in `data`, as a client started with a data directory
        /// keeps them.
        fn keep_receipts(&mut self, data: Arc<ClientDir>);
    }

    impl SimClientWorldInternals for World {
        fn keep_receipts(&mut self, data: Arc<ClientDir>) {
            self.insert_resource(ReceiptWriter::start(data));
        }
    }
}
