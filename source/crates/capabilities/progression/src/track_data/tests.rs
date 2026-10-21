use campfire_codec::Toml;

use super::*;

#[test]
fn thresholds_are_positive_and_strictly_ascending_as_built_and_as_read() {
    let thresholds = |levels: &[i64]| Thresholds::new(levels.iter().map(|&value| Num::int(value)));
    for refused in [
        &[][..],
        &[0],
        &[-5, 100],
        &[100, 100],
        &[300, 100],
        &[100, 300, 200],
    ] {
        assert_eq!(thresholds(refused), None, "{refused:?}");
    }
    assert_eq!(thresholds(&[1]).unwrap().get(), [Num::int(1)]);
    assert_eq!(
        thresholds(&[100, 101, 300]).unwrap().get(),
        [Num::int(100), Num::int(101), Num::int(300)]
    );
    let read = |text: &str| Toml::parse::<TrackData>(text);
    let refusal = |text: &str| read(text).unwrap_err().message().to_owned();
    assert_eq!(
        read("levels = [\"0.5\", 300]\nlevel = true").unwrap(),
        TrackData {
            levels: Thresholds::new([Num::HALF, Num::int(300)]).unwrap(),
            level: true,
        }
    );
    for refused in ["levels = []", "levels = [0]", "levels = [300, 100]"] {
        assert!(
            refusal(refused).starts_with("a track has a level 2"),
            "{refused}"
        );
    }
    // An integer past what a number holds fails before the order is checked.
    assert!(
        refusal("levels = [9223372036854775807]")
            .starts_with("9223372036854775807 is past what a number holds")
    );
}
