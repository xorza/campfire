use super::*;

#[test]
fn a_kind_holds_its_2_bits() {
    for bits in 0..PayloadKind::COUNT {
        assert_eq!(PayloadKind::new(bits).bits(), bits);
    }
}

#[test]
#[should_panic(expected = "a payload kind has 2 bits")]
fn a_kind_past_2_bits_is_refused() {
    let _ = PayloadKind::new(PayloadKind::COUNT);
}
