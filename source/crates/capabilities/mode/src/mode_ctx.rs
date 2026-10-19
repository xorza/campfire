use std::sync::Arc;

use campfire_script::{ApiError, Checked, Ctx};

use crate::game_map::GameMap;
use crate::mode_call::ModeCall;
use crate::mode_spec::ModeSpec;

/// What the mode reads of a `ctx`: its spec, which its part of the frame holds once it installs.
pub(crate) trait ModeCtx {
    /// The mode of the match, once it installed.
    fn mode_spec(&self) -> Option<Arc<ModeSpec>>;

    /// The mode of the match, which has one: the mode's own systems and calls run only in such a
    /// match.
    fn match_mode_spec(&self) -> Arc<ModeSpec>;

    /// The mode of the match; an error in a match with none.
    fn mode_spec_or_fail(&self) -> Checked<Arc<ModeSpec>>;

    /// `ctx.map`; an error in a match with no mode.
    fn mode_map_or_fail(&self) -> Checked<GameMap>;
}

impl ModeCtx for Ctx {
    fn mode_spec(&self) -> Option<Arc<ModeSpec>> {
        self.frame().extension::<ModeCall>().map(ModeCall::spec)
    }

    fn match_mode_spec(&self) -> Arc<ModeSpec> {
        self.mode_spec().expect("a match with a mode")
    }

    fn mode_spec_or_fail(&self) -> Checked<Arc<ModeSpec>> {
        self.mode_spec()
            .ok_or_else(|| ApiError::NoMode.fail().into())
    }

    fn mode_map_or_fail(&self) -> Checked<GameMap> {
        let map = self.frame().extension::<ModeCall>().map(ModeCall::map);
        map.ok_or_else(|| ApiError::NoMode.fail().into())
    }
}
