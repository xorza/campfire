use campfire_script::{ApiBuilder, ApiError, ApiHolder, Ctx, MemberDecl, ScriptPosition, Unit};

use crate::mode_navigation_data::ModeNavigationData;
use crate::navigation_column::NavigationColumn;
use crate::navigation_effect::NavigationEffect;

/// The script API of `navigation` beside its effect's calls: `unit.path`, and `ctx.teleport`.
#[derive(Debug)]
pub struct NavigationApi;

impl NavigationApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        let path = MemberDecl::field(
            ApiHolder::of::<Unit>(),
            "path",
            "the name of the path it walks, `()` with none",
        );
        api.bind(path, |unit: &mut Unit| NavigationColumn::path(unit));
        Self::register_forced(api);
    }

    /// `ctx.teleport`.
    fn register_forced(api: &mut ApiBuilder<'_>) {
        let teleport = MemberDecl::call(
            "teleport",
            &[&["unit", "position"]],
            "puts `unit` at `position` at once, or at the nearest place it may stand",
        );
        api.call(teleport, |ctx: &mut Ctx, of: Unit, to: ScriptPosition| {
            let teleport = NavigationEffect::teleport(ctx.view(), of.id, to.get());
            ctx.queue(teleport.map_err(ApiError::fail)?)
        })
        .data::<ModeNavigationData>(&[]);
    }
}
