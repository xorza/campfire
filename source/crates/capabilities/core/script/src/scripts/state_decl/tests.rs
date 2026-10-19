use std::num::NonZeroU32;

use campfire_codec::Toml;
use campfire_sim::TickRate;

use super::*;
use crate::script_host::error::ScriptError;
use crate::scripts::error::{CallError, FailureKind};
use crate::scripts::script_text::ScriptText;
use crate::scripts::script_value::script_list::ScriptList;
use crate::scripts::state_decl::synced_state_decl::SyncedStateDecl;
use crate::scripts::view::View;

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
            Some(StateValue::Text("pick".into())),
        ),
        ((StateType::Unit, None), Some(StateValue::Unit(None))),
        (
            (StateType::UnitList, None),
            Some(StateValue::UnitList(Arc::from([]))),
        ),
        ((StateType::Vec, None), Some(StateValue::Vec(Vec3::ZERO))),
        // Defaults not of the type.
        ((StateType::Int, text("4")), None),
        ((StateType::Num, text("half")), None),
        ((StateType::Bool, Some(StateDefault::Int(1))), None),
        ((StateType::Unit, Some(StateDefault::Int(0))), None),
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

#[test]
fn an_enum_starts_at_its_default_or_first_member_and_holds_only_its_members() {
    let members = || vec!["pick".to_owned(), "play".to_owned()];
    let text = |text: &str| Some(StateDefault::Text(text.to_owned()));
    let initial = |members, default| {
        StateDecl::enumeration(members, default).map(|decl| decl.initial().clone())
    };
    assert_eq!(
        initial(members(), None),
        Some(StateValue::Text("pick".into()))
    );
    assert_eq!(
        initial(members(), text("play")),
        Some(StateValue::Text("play".into()))
    );
    // No member, a member twice, and a default that is no member, or no text, fail.
    let twice = vec!["a".to_owned(), "a".to_owned()];
    for (members, default) in [
        (Vec::new(), None),
        (twice, None),
        (members(), text("end")),
        (members(), Some(StateDefault::Int(0))),
    ] {
        assert_eq!(initial(members, default), None);
    }
    // A member's text is its value; another text, or a value of another type, fails the call.
    let decl = StateDecl::enumeration(members(), None).unwrap();
    assert_eq!(decl.kind(), StateType::Enum);
    let play = ScriptValue::of(ScriptText::from("play"));
    assert_eq!(decl.take(&play).unwrap(), StateValue::Text("play".into()));
    let fails = |value: ScriptValue| {
        let error = decl.take(&value).unwrap_err();
        CallError::from_script(ScriptError::from_eval(error)).kind()
    };
    assert_eq!(
        fails(ScriptValue::of(ScriptText::from("end"))),
        FailureKind::Api(ApiError::NotAMember)
    );
    assert_eq!(
        fails(ScriptValue::of(1_i64)),
        FailureKind::Api(ApiError::WrongStateType)
    );
    assert!(decl.holds(&StateValue::Text("play".into())));
    assert!(!decl.holds(&StateValue::Text("end".into())));
    assert!(!decl.holds(&StateValue::Int(0)));
    // A field of another type holds its type's values alone, a text whatever it says.
    let string = StateDecl::new(StateType::String, None).unwrap();
    assert!(string.holds(&StateValue::Text("end".into())));
    assert!(!string.holds(&StateValue::Int(0)));
    // Data gives an enum its members as `values`, which no other type takes, and an enum
    // needs.
    let read = |text: &str| Toml::parse::<SyncedStateDecl>(text).map(|field| field.decl);
    let phase = read("type = \"enum\"\nvalues = [\"pick\", \"play\"]\ndefault = \"play\"");
    assert_eq!(phase.unwrap().initial(), &StateValue::Text("play".into()));
    assert!(read("type = \"int\"\nvalues = [\"a\"]").is_err());
    assert!(read("type = \"enum\"").is_err());
}

#[test]
fn a_list_of_scalars_holds_items_of_its_type_alone() {
    let list = |items: Vec<ScriptValue>| ScriptValue::of(items.into_iter().collect::<ScriptList>());
    let (one, two) = (ScriptValue::of(1_i64), ScriptValue::of(2_i64));
    let half = ScriptValue::of(Num::HALF);
    let yes = ScriptValue::of(true);
    let word = ScriptValue::of(ScriptText::from("a"));
    let take = |kind, value: &ScriptValue| StateValue::of_kind(kind, value);
    assert_eq!(
        take(StateType::IntList, &list(vec![one.clone(), two])),
        Some(StateValue::IntList(Arc::from([1, 2])))
    );
    // An integer is a `Num` too, in a list as alone.
    assert_eq!(
        take(StateType::NumList, &list(vec![one.clone(), half.clone()])),
        Some(StateValue::NumList(Arc::from([Num::ONE, Num::HALF])))
    );
    assert_eq!(
        take(StateType::BoolList, &list(vec![yes])),
        Some(StateValue::BoolList(Arc::from([true])))
    );
    assert_eq!(
        take(StateType::StringList, &list(vec![word.clone()])),
        Some(StateValue::TextList(Arc::from([Arc::from("a")])))
    );
    // An item of another type, or a value that is no list, fails.
    assert_eq!(
        take(StateType::IntList, &list(vec![one.clone(), half])),
        None
    );
    assert_eq!(take(StateType::BoolList, &list(vec![word])), None);
    assert_eq!(take(StateType::IntList, &one), None);
    // Each list starts empty, and reads back as the list it holds.
    let view = View::new(TickRate::new(NonZeroU32::new(30).unwrap()), |_| {});
    let back = StateValue::IntList(Arc::from([1, 2])).to_value(&view);
    let items: Vec<_> = back
        .into_list()
        .unwrap()
        .into_iter()
        .map(|item| item.as_int())
        .collect();
    assert_eq!(items, [Some(1), Some(2)]);
    let empty = StateDecl::new(StateType::StringList, None).unwrap();
    assert_eq!(empty.initial(), &StateValue::TextList(Arc::from([])));
}
