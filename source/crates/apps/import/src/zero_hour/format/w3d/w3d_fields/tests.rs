use super::*;
use crate::zero_hour::format::error::CutShort;
use crate::zero_hour::format::w3d::w3d_fields::samples::name;

#[test]
fn fields_read_little_endian_and_names_up_to_their_nul() {
    let bytes = [
        &[0x34, 0x12][..],
        &0xDEAD_BEEF_u32.to_le_bytes(),
        &1.5_f32.to_le_bytes(),
        &name("MiXeD", 8),
        &[7, 0, 0, 0, 9, 0, 0, 0],
    ]
    .concat();
    let mut fields = W3dFields::of(&bytes);
    assert_eq!(fields.cursor.u16().unwrap(), 0x1234);
    assert_eq!(fields.cursor.u32().unwrap(), 0xDEAD_BEEF);
    assert_eq!(fields.cursor.f32().unwrap(), 1.5);
    assert_eq!(fields.name(8).unwrap(), "mixed");
    assert_eq!(fields.u32s().unwrap(), [7, 9]);
    assert_eq!(fields.cursor.u32(), Err(CutShort));
    // A name of no NUL fills its field; one past ASCII is refused; a tail not whole is short.
    assert_eq!(W3dFields::of(b"ABCD").name(4).unwrap(), "abcd");
    assert!(matches!(
        W3dFields::of(&[0xE9, 0]).name(2),
        Err(W3dError::NotAscii)
    ));
    assert!(matches!(
        W3dFields::of(&[1, 2, 3]).records::<2>(),
        Err(W3dError::Short)
    ));
    assert!(matches!(
        W3dFields::of(&[1, 2, 3]).u32s(),
        Err(W3dError::Short)
    ));
}
