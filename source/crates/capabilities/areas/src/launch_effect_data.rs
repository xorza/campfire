use std::sync::Arc;

use campfire_actions::{
    EffectError, EffectResolveCx, EffectRuleCx, LaunchId, LaunchLists, ListedAt, ListedEffect,
    ListedEffects, ListedSpec,
};
use campfire_codec::{TomlError, TomlTable, TomlValue};
use campfire_common::DeclaredName;
use campfire_declare::{BookError, Listed, ReadValue, SectionReaders};
use campfire_deliveries::Delivering;
use campfire_script::{CallError, Frame, NameKind, View};
use campfire_sim::Capability;
use campfire_values::{Number, UnitType};
use serde::Deserialize;

use crate::area_data::AreaData;
use crate::areas_effect::AreasEffect;
use crate::error::AreaDeliveryError;

/// `launch = { area, on_hit, on_end }`: an area of the type `area` of the action's package, a
/// type of the area section, which runs its own lists: `on_hit` on each unit it hits, which it
/// reaches, and `on_end` as it ends, which reaches none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchEffectData {
    pub area: DeclaredName,
    pub lists: LaunchLists,
}

/// A listed launch: its area type, and the lists it runs, as the book of lists holds them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LaunchEffectSpec {
    area: UnitType,
    launch: LaunchId,
}

/// The fields of a launch's table beside its lists.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LaunchFields {
    area: DeclaredName,
}

/// Its lists read with the release's readers, which read their effects.
impl ReadValue for LaunchEffectData {
    fn read_value(value: TomlValue, readers: &SectionReaders) -> Result<Self, TomlError> {
        let mut table = value.read::<TomlTable>()?;
        let lists = LaunchLists::take(&mut table, readers)?;
        let LaunchFields { area } = table.read()?;
        Ok(Self { area, lists })
    }
}

impl Listed for LaunchEffectData {
    type Set = ListedEffects;

    const KEY: &'static str = "launch";

    fn into_listed(self) -> Arc<dyn ListedEffect> {
        Arc::new(self)
    }
}

impl ListedEffect for LaunchEffectData {
    fn needs(&self) -> Option<Capability> {
        Some(Capability::Areas)
    }

    fn numbers(&self) -> Vec<&Number> {
        Vec::new()
    }

    fn duration(&self) -> Option<&Number> {
        None
    }

    fn modifier(&self) -> Option<&DeclaredName> {
        None
    }

    fn lists(&self) -> Option<&LaunchLists> {
        Some(&self.lists)
    }

    fn takes_point(&self) -> bool {
        false
    }

    fn rule(&self, cx: &dyn EffectRuleCx) -> Result<(), BookError> {
        let Some(area) = cx.package_type(&self.area) else {
            return Err(cx.unknown(NameKind::UnitType, &self.area));
        };
        if area.section::<AreaData>().is_none() {
            return Err(cx
                .at()
                .broken(AreaDeliveryError::NotArea(self.area.clone())));
        }
        cx.lists(&self.lists)
    }

    fn resolve(&self, cx: &mut dyn EffectResolveCx) -> Result<Arc<dyn ListedSpec>, EffectError> {
        let area = cx.names().package_type(&self.area)?;
        Ok(Arc::new(LaunchEffectSpec {
            area,
            launch: cx.launch(&self.lists)?,
        }))
    }
}

impl ListedSpec for LaunchEffectSpec {
    /// Queues an area of its type, of the call's action at its rank from its acting unit, which
    /// runs the launch's lists, where the unit stands as `view` reads it; none when the unit is
    /// gone.
    fn queue(&self, at: ListedAt, frame: &mut Frame, view: &View) -> Result<(), CallError> {
        let (Some(source), Some(action)) = (frame.acting(), frame.action()) else {
            unreachable!("a list runs for its action's acting unit")
        };
        // A unit gone, as a source despawned while its delivery flew, has no place: its area
        // lands nothing, as one whose source is gone does.
        let Some(row) = view.row(at.unit(frame)) else {
            return Ok(());
        };
        let by = Delivering {
            source,
            action,
            rank: frame.rank(),
            start: frame.start(),
            launch: Some(self.launch),
        };
        frame.queue(AreasEffect {
            id: None,
            by,
            unit_type: self.area,
            pos: row.pos,
        });
        Ok(())
    }
}
