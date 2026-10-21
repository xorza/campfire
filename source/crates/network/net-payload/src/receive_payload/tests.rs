use super::*;

#[test]
fn either_receive_payload_freezes_without_a_copy() {
    let mutable = BytesMut::from(&[1, 2, 3][..]);
    let allocation = mutable.as_ptr();
    assert_eq!(ReceivePayload::from(mutable).freeze().as_ptr(), allocation);

    // A frozen payload whose allocation is shared stays a view of it.
    let bytes = Bytes::from(std::vec![1, 2, 3]);
    let shared = bytes.clone();
    assert_eq!(
        ReceivePayload::from(bytes).freeze().as_ptr(),
        shared.as_ptr()
    );
}

#[test]
fn a_receive_payload_becomes_mutable_with_a_copy_only_when_frozen_and_shared() {
    let mutable = BytesMut::from(&[1, 2, 3][..]);
    let allocation = mutable.as_ptr();
    assert_eq!(
        ReceivePayload::from(mutable).into_mut().as_ptr(),
        allocation
    );

    let unique = Bytes::from(std::vec![1, 2, 3]);
    let allocation = unique.as_ptr();
    assert_eq!(ReceivePayload::from(unique).into_mut().as_ptr(), allocation);

    let bytes = Bytes::from(std::vec![1, 2, 3]);
    let shared = bytes.clone();
    let payload = ReceivePayload::from(bytes).into_mut();
    assert_ne!(payload.as_ptr(), shared.as_ptr());
    assert_eq!(payload.as_ref(), shared.as_ref());
}

#[test]
fn receive_payloads_compare_by_their_bytes_in_either_form() {
    let mutable = ReceivePayload::from(BytesMut::from(&[1, 2, 3][..]));
    assert_eq!(
        mutable,
        ReceivePayload::from(Bytes::from_static(&[1, 2, 3]))
    );
    assert_ne!(
        mutable,
        ReceivePayload::from(Bytes::from_static(&[1, 2, 4]))
    );
}
