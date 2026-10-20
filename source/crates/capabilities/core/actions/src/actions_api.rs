use campfire_declarations::ListedSet;
use campfire_script::{ApiBuilder, ApiVersion, Status};

use crate::action_data::ActionData;
use crate::listed_effect::ListedEffects;
use crate::listed_effect::effect_data::EffectData;
use crate::planned_effect::PlannedEffect;

/// The script API of the action pipeline: what a unit's actions give it, and the fields of an
/// action's data that every kind reads.
#[derive(Debug)]
pub(crate) struct ActionsApi;

impl ActionsApi {
    pub(crate) fn register(api: &mut ApiBuilder<'_>) {
        api.fields(
            ActionData::TABLE,
            ApiVersion::FIRST,
            ActionData::pipeline_keys(),
        )
        .key(
            ListedEffects::TABLE,
            EffectData::TO,
            Status::Runs(ApiVersion::FIRST),
        );
        for planned in PlannedEffect::ALL {
            api.key(ListedEffects::TABLE, planned.name(), Status::Planned);
        }
    }
}
