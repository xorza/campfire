use campfire_script::{ScriptHandle, ScriptValue, View};
use campfire_sim::StableId;
use campfire_values::Hit;

/// A hit as a script holds it, `Hit` in scripts: read only.
#[derive(Debug, Clone)]
pub struct HitHandle {
    hit: Hit,
    view: View,
}

impl HitHandle {
    pub const fn new(hit: Hit, view: View) -> Self {
        Self { hit, view }
    }

    pub const fn hit(&self) -> &Hit {
        &self.hit
    }

    /// The unit `id` names as the script view holds it, `()` with none or one gone.
    pub fn unit(&self, id: Option<StableId>) -> ScriptValue {
        self.view.unit_value(id)
    }
}

impl ScriptHandle for HitHandle {
    const NAME: &'static str = "Hit";
    const TITLE: &'static str = "Hit `hit`";
}
