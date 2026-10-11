use campfire_common::internals::{AllocAudit, CountingAllocator};
use campfire_quic::internals::ThreadFixture;

/// The most packets `quinn-proto` 0.11.19 sends before its first key update, which it draws from
/// 10..1000 so that a peer that cannot update fails early, and which allocates the next keys. The
/// next one comes at the cipher's confidentiality limit, millions of packets later.
const FIRST_KEY_UPDATE: usize = 1_000;

/// The server thread's pass over one read of a slot of 1, 3 or 8 datagrams that GRO joined,
/// which serves and acknowledges them. It allocates twice for the slot, both in `RecvBatch`:
/// the copy into a `BytesMut`, which `quinn-proto` takes by value, and the shared header `bytes`
/// makes at its first split. The warmup passes the first key update.
#[test]
fn a_pass_over_a_slot_allocates_only_its_copy_and_its_shared_header() {
    for datagrams in [1, 3, 8] {
        let mut fixture = ThreadFixture::connected();
        AllocAudit::new()
            .warmup(FIRST_KEY_UPDATE.div_ceil(datagrams))
            .budget(2)
            .run(|| {
                let slots = CountingAllocator::excluded(|| fixture.send(datagrams));
                assert_eq!(slots, 1, "{datagrams} datagrams in one slot");
                fixture.pass();
                assert_eq!(CountingAllocator::excluded(|| fixture.answer()), datagrams);
            });
    }
}
