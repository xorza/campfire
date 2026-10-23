use super::*;

/// A deallocated id is handed out again before a new one.
#[test]
fn a_deallocated_id_is_reused() {
    let mut allocator = RoomAllocator::default();
    let [a, b] = [allocator.allocate(), allocator.allocate()];
    assert_eq!([a, b], [RoomId(0), RoomId(1)]);
    allocator.deallocate(a);
    assert_eq!(allocator.allocate(), RoomId(0));
    assert_eq!(allocator.allocate(), RoomId(2));
}

/// Every id of the `u16` space is handed out, the last 65,535, and none past it.
#[test]
#[should_panic(expected = "at most 65,536 rooms live at once")]
fn the_65536th_room_is_the_last() {
    let mut allocator = RoomAllocator::default();
    let last = (0..=u16::MAX).map(|_| allocator.allocate()).last();
    assert_eq!(last, Some(RoomId(u16::MAX)));
    allocator.allocate();
}
