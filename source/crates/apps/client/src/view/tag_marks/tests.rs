use super::*;

/// Two marks: of tag 3, over 1 s, stilling clips; of tag 5, over no time.
fn marks() -> TagMarks {
    TagMarks(vec![
        ResolvedMark {
            tag: Some(Tag::new(3)),
            fade_seconds: 1.0,
            stills: true,
        },
        ResolvedMark {
            tag: Some(Tag::new(5)),
            fade_seconds: 0.0,
            stills: false,
        },
    ])
}

#[test]
fn a_mark_fades_in_while_its_tag_is_held_and_out_after_and_tints_by_its_step() {
    let marks = marks();
    let (first, second) = (TagSet::from([Tag::new(3)]), TagSet::from([Tag::new(5)]));
    let mut fades = [0.0; 2];
    // 0.1 s of 1 s held: 0.1 of the fade, 3 of 30 steps, the third from 0 at 2.
    let tint = marks.step(&mut fades, first, 0.1);
    assert_eq!(tint, Some(Tint::Mark { mark: 0, step: 2 }));
    // 0.25 s more: 0.35, 10.5 steps rounded up to 11, at 10.
    let tint = marks.step(&mut fades, first, 0.25);
    assert!((fades[0] - 0.35).abs() < 1e-6, "{fades:?}");
    assert_eq!(tint, Some(Tint::Mark { mark: 0, step: 10 }));
    // 2 s more: whole, the last step, 29.
    assert_eq!(
        marks.step(&mut fades, first, 2.0),
        Some(Tint::Mark { mark: 0, step: 29 })
    );
    // The tag of the second, which fades at once, while the first fades out: the first still
    // shows, 0.5 s out of 1, at 15 steps, 14; the second waits behind it.
    let tint = marks.step(&mut fades, second, 0.5);
    assert_eq!(fades, [0.5, 1.0]);
    assert_eq!(tint, Some(Tint::Mark { mark: 0, step: 14 }));
    // Once the first is out, the second shows whole; and it goes at once with its tag.
    assert_eq!(
        marks.step(&mut fades, second, 0.5),
        Some(Tint::Mark { mark: 1, step: 29 })
    );
    assert_eq!(marks.step(&mut fades, TagSet::default(), 0.01), None);
    assert_eq!(fades, [0.0, 0.0]);
}

#[test]
fn a_mark_stills_a_unit_of_its_tag_only_when_it_says_so() {
    let marks = marks();
    assert!(marks.stills(TagSet::from([Tag::new(3)])));
    assert!(!marks.stills(TagSet::from([Tag::new(5)])));
    assert!(!marks.stills(TagSet::default()));
    assert!(marks.holds(TagSet::from([Tag::new(5)])));
    assert!(!marks.holds(TagSet::from([Tag::new(4)])));
    // A mark of a tag the match lacks marks nothing.
    let lacked = TagMarks(vec![ResolvedMark {
        tag: None,
        fade_seconds: 1.0,
        stills: true,
    }]);
    assert!(!lacked.stills(TagSet::from([Tag::new(3)])));
    assert_eq!(
        lacked.step(&mut [0.0], TagSet::from([Tag::new(3)]), 1.0),
        None
    );
}
