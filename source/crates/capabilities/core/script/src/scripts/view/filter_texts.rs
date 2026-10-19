use crate::scripts::error::ApiError;
use crate::scripts::script_text::ScriptText;
use crate::scripts::view::filter::Filter;
use crate::scripts::view::tag_names::TagNames;

/// The filters scripts gave as text, each as the match's tags resolve it, so a literal a call
/// site gives again resolves once: Rhai passes a literal as the one shared text its script holds,
/// which a lookup finds by identity before it compares any text. It keeps at most `HELD`, as a
/// script may build a new text at each call; a text past them resolves at each call
/// (02-engine-core.md, A filter's text resolves once).
#[derive(Debug, Default)]
pub(crate) struct FilterTexts {
    held: Vec<(ScriptText, Filter)>,
}

impl FilterTexts {
    /// The texts it keeps at most: more than a mode's scripts give as literals.
    const HELD: usize = 64;

    /// The filter `text` names, with its tags among `tags`, which it keeps the first time.
    pub(crate) fn filter(
        &mut self,
        text: &ScriptText,
        tags: &TagNames,
    ) -> Result<Filter, ApiError> {
        let same = self.held.iter().find(|(held, _)| held.same(text));
        let equal = || self.held.iter().find(|(held, _)| held == text);
        if let Some(&(_, filter)) = same.or_else(equal) {
            return Ok(filter);
        }
        let filter = Filter::parse(text, tags)?;
        if self.held.len() < Self::HELD {
            self.held.push((text.clone(), filter));
        }
        Ok(filter)
    }

    /// Forgets every text, as the match's tags changed.
    pub(crate) fn clear(&mut self) {
        self.held.clear();
    }
}

#[cfg(test)]
mod tests {
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
        assert!(texts.filter(&wrong, &tags).is_err());
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
}
