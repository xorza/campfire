use campfire_math::Num;

use super::*;

#[test]
fn a_pick_reads_as_a_name_and_a_number() {
    let read = |text: &str| text.parse::<OptionValue>();
    let pick = |name: &str, value| OptionValue {
        name: DeclaredName::new(name).unwrap(),
        value,
    };
    assert_eq!(
        read("start_money=50000"),
        Ok(pick("start_money", Scalar::Int(50000)))
    );
    assert_eq!(
        read("speed=0.5"),
        Ok(pick("speed", Scalar::Decimal(Num::HALF)))
    );
    assert_eq!(read("start_money"), Err(OptionValueError::NoValue));
    assert_eq!(
        read("Start=5"),
        Err(OptionValueError::Name("Start".to_owned()))
    );
    assert_eq!(
        read("start_money=lots"),
        Err(OptionValueError::Value("lots".to_owned()))
    );
}
