use campfire_script::{ApiBuilder, ApiHolder, MaybeName, MemberDecl, ScriptHandle, View};

use crate::match_end::MatchResult;

/// How a match ended as a script holds it, `MatchResult` in scripts: read only.
#[derive(Debug, Clone)]
pub(crate) struct ResultHandle {
    result: MatchResult,
    view: View,
}

impl ResultHandle {
    pub(crate) const fn new(result: MatchResult, view: View) -> Self {
        Self { result, view }
    }

    /// The `MatchResult` handle's fields.
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let field =
            |name, description| MemberDecl::field(ApiHolder::of::<Self>(), name, description);
        api.ty::<Self>()
            .bind(
                field("winner", "the team that won, `()` for a draw"),
                |r: &mut Self| {
                    let winner = match r.result {
                        MatchResult::Won(team) => Some(r.view.names().name(team)),
                        MatchResult::Draw => None,
                    };
                    MaybeName::from(winner)
                },
            )
            .bind(
                field("is_draw", "whether the match ended with no winner"),
                |r: &mut Self| r.result == MatchResult::Draw,
            );
    }
}

impl ScriptHandle for ResultHandle {
    const NAME: &'static str = "MatchResult";
    const TITLE: &'static str = "A match's result";
}
