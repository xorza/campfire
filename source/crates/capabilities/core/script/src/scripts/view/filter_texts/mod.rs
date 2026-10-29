use campfire_names::Filter;
use campfire_names::TagNames;

use crate::scripts::error::ApiError;
use crate::scripts::script_text::ScriptText;

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
        let filter = Filter::parse(text, tags).map_err(ApiError::Unknown)?;
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
mod tests;
