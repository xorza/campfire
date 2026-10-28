use campfire_codec::Toml;

use super::*;

#[test]
fn a_share_writes_what_it_reads_back() {
    /// A field of data that holds a share.
    #[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
    struct Field {
        share: Share,
    }
    // None and all write as integers, a part as its digits of no trailing zero.
    for (parts, written) in [
        (0, "share = 0\n"),
        (1_000_000_000, "share = 1\n"),
        (200_000, "share = \"0.0002\"\n"),
        (833_334, "share = \"0.000833334\"\n"),
        (500_000_000, "share = \"0.5\"\n"),
    ] {
        let field = Field {
            share: Share::billionths(parts).unwrap(),
        };
        assert_eq!(Toml::write(&field), written);
        assert_eq!(Toml::parse::<Field>(written).unwrap(), field);
    }
    assert_eq!(Share::billionths(1_000_000_001), None);
}

#[test]
fn a_share_is_exactly_the_decimal_data_writes() {
    /// A field of data that holds a share.
    #[derive(Debug, Deserialize)]
    struct Field {
        share: Share,
    }
    let share = |text: &str| Share::parse(text);
    // 0.7 has no exact binary value; as a fraction of 10 it gives 70% of 300 exactly, and
    // 0.333 of 1000 is 333.
    assert_eq!(share("0.7").unwrap().of(300), 210);
    assert_eq!(share("0.333").unwrap().of(1000), 333);
    assert_eq!(share("0.999999999").unwrap().of(1_000_000_000), 999_999_999);
    assert_eq!(share("1.0").unwrap().of(7), 7);
    assert_eq!(share("0").unwrap().of(7), 0);
    // Rounded down, toward −∞: 0.5 of 7 is 3, of −7 is −4; of the least step of a number,
    // ε, 0 and −ε.
    let half = share("0.5").unwrap();
    assert_eq!((half.of(7), half.of(-7)), (3, -4));
    assert_eq!(half.of_num(Num::EPSILON), Num::ZERO);
    assert_eq!(half.of_num(-Num::EPSILON), -Num::EPSILON);
    // 0.35 of 100 is 35, which it covers, and not 35 and the least step past it; 0.7 of 1, which
    // has no exact binary value, covers 0.7 rounded down to a number's bits, not rounded up.
    let much = share("0.35").unwrap();
    assert!(much.covers(Num::int(35), Num::int(100)));
    assert!(!much.covers(Num::int(35) + Num::EPSILON, Num::int(100)));
    let seven = share("0.7").unwrap();
    assert!(seven.covers(seven.of_num(Num::ONE), Num::ONE));
    assert!(!seven.covers(seven.of_num(Num::ONE) + Num::EPSILON, Num::ONE));
    for refused in [
        "1.5",
        "2",
        "-0.5",
        "0.",
        ".5",
        "0.1234567891",
        "0.5x",
        "1.01",
    ] {
        assert_eq!(share(refused), None, "{refused}");
    }
    let read = |text: &str| Toml::parse::<Field>(text).map(|field| field.share).ok();
    let of = |text: &str| read(text).map(|share| share.of(100));
    assert_eq!(of("share = 1"), Some(100));
    assert_eq!(of("share = 0"), Some(0));
    assert_eq!(of("share = \"0.25\""), Some(25));
    assert_eq!(of("share = 2"), None);
    // One share however data writes it.
    assert_eq!(share("0.5"), share("0.50"));
    assert_eq!("0.35".parse(), Ok(share("0.35").unwrap()));
    assert_eq!("1.5".parse::<Share>(), Err(ShareError));
    assert_eq!(share("1.0"), Some(Share::ALL));
    assert_eq!(read("share = 1"), Some(Share::ALL));
}
