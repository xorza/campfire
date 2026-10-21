use campfire_values::{Article, NameKind, NameScope};

use super::*;

#[test]
fn a_text_given_again_resolves_once_and_past_the_bound_at_each_call() {
    let tags = TagNames::default();
    let mut texts = FilterTexts::default();
    let enemies = ScriptText::from("enemies");
    let allies = ScriptText::from("allies");
    let parsed = |text: &str| Filter::parse(text, &tags).unwrap();
    // The literal resolves once: the same shared text finds its entry, and a new text of the
    // same letters finds it too, and adds none.
    assert_eq!(texts.filter(&enemies, &tags), Ok(parsed("enemies")));
    assert_eq!(texts.filter(&enemies, &tags), Ok(parsed("enemies")));
    let again = ScriptText::from("enemies");
    assert!(!again.same(&enemies));
    assert_eq!(texts.filter(&again, &tags), Ok(parsed("enemies")));
    assert_eq!(texts.filter(&allies, &tags), Ok(parsed("allies")));
    assert_ne!(parsed("enemies"), parsed("allies"));
    assert_eq!(texts.held.len(), 2);
    // A text that names no filter is kept out, and fails each time.
    let wrong = ScriptText::from("nobody");
    let unknown = ApiError::Unknown(NameKind::new("filter", Article::A, NameScope::Match));
    assert_eq!(texts.filter(&wrong, &tags), Err(unknown));
    assert_eq!(texts.held.len(), 2);
    // Past `HELD` texts, a new one still resolves, and is not kept.
    for _ in texts.held.len()..FilterTexts::HELD {
        texts
            .held
            .push((ScriptText::from("hostiles"), parsed("hostiles")));
    }
    let neutral = ScriptText::from("neutrals");
    assert_eq!(texts.filter(&neutral, &tags), Ok(parsed("neutrals")));
    assert_eq!(texts.held.len(), FilterTexts::HELD);
    texts.clear();
    assert!(texts.held.is_empty());
}
