use campfire_common::internals::{AllocAudit, CountingAllocator};
use campfire_quic::internals::ThreadFixture;

/// The most packets `quinn-proto` 0.11.19 sends before its first key update, which it draws from
/// 10..1000 so that a peer that cannot update fails early, and which allocates the next keys. The
/// next one comes at the cipher's confidentiality limit, millions of packets later.
const FIRST_KEY_UPDATE: usize = 1_000;

/// The server thread's pass over one read of a slot of 1, 3 or 8 datagrams that GRO joined,
/// which serves and acknowledges them, allocates nothing: the slot's copy goes into an arena of
/// `RecvBatch`'s ring, which the next lap reuses, as the fixture's frame drops each datagram
/// before it. The warmup, sized for the first key update, also covers the ring's first lap, in
/// which each arena's first split allocates the shared header of `bytes`: 4 arenas of 256 KiB
/// fill in about 109 frames of 8 datagrams, inside a warmup of 125. A larger arena fails here.
#[test]
fn a_pass_over_a_slot_allocates_nothing() {
    for datagrams in [1, 3, 8] {
        let mut fixture = ThreadFixture::connected();
        AllocAudit::new()
            .warmup(FIRST_KEY_UPDATE.div_ceil(datagrams))
            .run(|| {
                let slots = CountingAllocator::excluded(|| fixture.send(datagrams));
                assert_eq!(slots, 1, "{datagrams} datagrams in one slot");
                fixture.pass();
                assert_eq!(CountingAllocator::excluded(|| fixture.answer()), datagrams);
            });
    }
}
