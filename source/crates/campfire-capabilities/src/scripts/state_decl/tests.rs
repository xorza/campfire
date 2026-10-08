use super::*;

#[test]
fn a_field_starts_at_its_default_or_its_types_zero() {
    let text = |text: &str| Some(StateDefault::Text(text.to_owned()));
    let cases = [
        (
            (StateType::Int, Some(StateDefault::Int(4))),
            Some(StateValue::Int(4)),
        ),
        ((StateType::Int, None), Some(StateValue::Int(0))),
        (
            (StateType::Num, Some(StateDefault::Int(2))),
            Some(StateValue::Num(Num::from_int(2).unwrap())),
        ),
        (
            (StateType::Num, text("0.5")),
            Some(StateValue::Num(Num::HALF)),
        ),
        (
            (StateType::Bool, Some(StateDefault::Bool(true))),
            Some(StateValue::Bool(true)),
        ),
        (
            (StateType::String, text("pick")),
            Some(StateValue::Text("pick".to_owned())),
        ),
        ((StateType::Entity, None), Some(StateValue::Entity(None))),
        (
            (StateType::EntityList, None),
            Some(StateValue::EntityList(Vec::new())),
        ),
        (
            (StateType::Pos, None),
            Some(StateValue::Pos(Position::ORIGIN)),
        ),
        ((StateType::Vec, None), Some(StateValue::Vec(Vec3::ZERO))),
        // Defaults not of the type.
        ((StateType::Int, text("4")), None),
        ((StateType::Num, text("half")), None),
        ((StateType::Bool, Some(StateDefault::Int(1))), None),
        ((StateType::Entity, Some(StateDefault::Int(0))), None),
    ];
    for ((kind, default), initial) in cases {
        let declared = format!("{kind:?} {default:?}");
        let decl = StateDecl::new(kind, default);
        // A field's type is its first value's.
        assert!(
            decl.as_ref().is_none_or(|decl| decl.kind() == kind),
            "{declared}"
        );
        assert_eq!(
            decl.map(|decl| decl.initial().clone()),
            initial,
            "{declared}"
        );
    }
}
