use campfire_codec::Toml;

use super::*;

#[test]
fn picks_are_a_place_of_each_option_in_the_order_of_their_names() {
    let option = |text: &str| Toml::parse::<OptionData>(text).unwrap();
    let options: BTreeMap<DeclaredName, OptionData> = [
        (
            "money",
            option("values = [5000, 10000, 50000]\ndefault = 10000"),
        ),
        ("bounty", option("values = [3, 40]\ndefault = 40")),
    ]
    .map(|(name, option)| (DeclaredName::new(name).unwrap(), option))
    .into();
    // By name, the bounty first, then the money.
    let named = |picks: &OptionPicks| {
        let values = picks.values(&options);
        values
            .map(|(name, value)| (name.as_str().to_owned(), value))
            .collect::<Vec<_>>()
    };
    let defaults = OptionPicks::defaults(&options);
    assert_eq!(defaults.places(), [1, 1]);
    assert_eq!(
        named(&defaults),
        [
            ("bounty".to_owned(), Scalar::Int(40)),
            ("money".to_owned(), Scalar::Int(10000))
        ]
    );
    let rich = OptionPicks::checked(&options, &[0, 2]).unwrap();
    assert_eq!(
        named(&rich),
        [
            ("bounty".to_owned(), Scalar::Int(3)),
            ("money".to_owned(), Scalar::Int(50000))
        ]
    );
    // Another count than the options', and a place past an option's values, are refused.
    assert_eq!(
        OptionPicks::checked(&options, &[0]),
        Err(PickError::Count {
            given: 1,
            options: 2
        })
    );
    assert_eq!(
        OptionPicks::checked(&options, &[0, 3]),
        Err(PickError::Place {
            option: DeclaredName::new("money").unwrap(),
            place: 3
        })
    );
    // By value: the other options keep their defaults; an option the mode lacks, a value
    // its option lacks, and one option twice, are refused.
    let value = |text: &str| text.parse::<OptionValue>().unwrap();
    let rich = OptionPicks::named(&options, &[value("money=50000")]).unwrap();
    assert_eq!(rich.places(), [1, 2]);
    let both = [value("bounty=3"), value("money=5000")];
    assert_eq!(
        OptionPicks::named(&options, &both).unwrap().places(),
        [0, 0]
    );
    let refused = |values: &[OptionValue]| OptionPicks::named(&options, values).unwrap_err();
    let name = |name: &str| DeclaredName::new(name).unwrap();
    assert_eq!(
        refused(&[value("speed=2")]),
        PickError::Unknown(name("speed"))
    );
    assert_eq!(
        refused(&[value("money=7")]),
        PickError::NoValue {
            option: name("money"),
            value: Scalar::Int(7)
        }
    );
    assert_eq!(
        refused(&[value("money=5000"), value("money=10000")]),
        PickError::Twice(name("money"))
    );
    let none = BTreeMap::<DeclaredName, OptionData>::new();
    assert!(OptionPicks::defaults(&none).places().is_empty());
    assert!(OptionPicks::checked(&none, &[]).is_ok());
}
