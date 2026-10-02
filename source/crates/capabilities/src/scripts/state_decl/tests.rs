use super::*;

#[test]
fn a_field_starts_at_its_default_or_its_types_zero() {
    let decl = |kind, default| StateDecl::new(kind, default).map(|decl| decl.initial);
    let text = |text: &str| Some(StateDefault::Text(text.to_owned()));
    let cases = [
        (
            decl(StateType::Int, Some(StateDefault::Int(4))),
            Some(StateValue::Int(4)),
        ),
        (decl(StateType::Int, None), Some(StateValue::Int(0))),
        (
            decl(StateType::Num, Some(StateDefault::Int(2))),
            Some(StateValue::Num(Num::from_int(2).unwrap())),
        ),
        (
            decl(StateType::Num, text("0.5")),
            Some(StateValue::Num(Num::from_bits(1 << 23))),
        ),
        (
            decl(StateType::Bool, Some(StateDefault::Bool(true))),
            Some(StateValue::Bool(true)),
        ),
        (
            decl(StateType::String, text("pick")),
            Some(StateValue::Text("pick".to_owned())),
        ),
        (
            decl(StateType::Entity, None),
            Some(StateValue::Entity(None)),
        ),
        (
            decl(StateType::EntityList, None),
            Some(StateValue::EntityList(Vec::new())),
        ),
        (
            decl(StateType::Pos, None),
            Some(StateValue::Pos(Position::ORIGIN)),
        ),
        (
            decl(StateType::Vec, None),
            Some(StateValue::Vec(Vec3::ZERO)),
        ),
        // Defaults not of the type.
        (decl(StateType::Int, text("4")), None),
        (decl(StateType::Num, text("half")), None),
        (decl(StateType::Bool, Some(StateDefault::Int(1))), None),
        (decl(StateType::Entity, Some(StateDefault::Int(0))), None),
    ];
    for (at, (decl, initial)) in cases.into_iter().enumerate() {
        assert_eq!(decl, initial, "case {at}");
    }
}
