use campfire_actions::{ActionsData, LoadedAction};
use campfire_declarations::{BookCx, BookError, ById, ByIdBuilder, PerId};
use campfire_names::UnknownName;
use campfire_stats::{ModifierAppliers, ModifierBook, Way};
use campfire_values::{ActionId, ModifierId, Named};

use crate::research_data::{ResearchData, ResearchHolder};

/// What a research action runs: the modifier it gives, and who holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResearchSpec {
    pub modifier: ModifierId,
    pub holder: ResearchHolder,
}

impl PerId<ActionId> for ResearchSpec {
    const SCRIPTS_ONLY: bool = false;
}

impl ResearchSpec {
    /// What each research gives: a modifier of its package, and who holds it; an error for one it
    /// does not have. A match with no stats holds no research, as its section needs them.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<ActionId, Self>, BookError> {
        let modifiers = cx.book_if::<ModifierBook>()?;
        let mut book = ByIdBuilder::default();
        for action in LoadedAction::all(cx)? {
            if let Some(spec) = Self::of(&action, modifiers)? {
                book.set(action.action, spec);
            }
        }
        Ok(book.build())
    }

    /// What `action` gives, of the modifiers `modifiers`, when it is a research; an error for a
    /// modifier of its package it does not have.
    pub(crate) fn of(
        action: &LoadedAction<'_>,
        modifiers: Option<&ModifierBook>,
    ) -> Result<Option<Self>, BookError> {
        let Some(data) = action.section::<ResearchData>() else {
            return Ok(None);
        };
        let named = modifiers.and_then(|modifiers| {
            modifiers
                .names()
                .named((action.package, data.modifier.as_str()))
        });
        let modifier = named
            .ok_or_else(|| action.unknown(UnknownName::new(ModifierId::NAMES, &data.modifier)))?;
        Ok(Some(Self {
            modifier,
            holder: data.holder,
        }))
    }

    /// Adds the modifier each research gives as applied by no action, as its player and its
    /// producer hold it, so the load checks it reads no param that way does not give.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    pub(crate) fn applied_ways(
        cx: &BookCx<'_>,
        appliers: &mut ModifierAppliers,
    ) -> Result<(), BookError> {
        for (package, source) in cx.packages() {
            let actions = source.content.get::<ActionsData>().into_iter();
            for (_, action) in actions.flat_map(|actions| &actions.0) {
                if let Some(research) = action.section::<ResearchData>() {
                    appliers.apply(package, research.modifier.as_str(), Way::NoAction);
                }
            }
        }
        Ok(())
    }
}
