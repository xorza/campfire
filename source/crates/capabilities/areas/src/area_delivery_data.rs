use std::sync::Arc;

use campfire_actions::{DeliverySection, DeliverySections, Fan};
use campfire_common::DeclaredName;
use campfire_declarations::{Listed, ListedSet};
use campfire_script::{ApiBuilder, DataSection, DataTable};
use campfire_script::{ApiVersion, Status};
use campfire_sim::Capability;
use serde::{Deserialize, Serialize};

/// An action's delivery of an area, `delivery = { area }`: the area type of its package it lands,
/// one at a time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AreaDeliveryData {
    pub area: DeclaredName,
}

impl DeliverySection for AreaDeliveryData {
    fn capability(&self) -> Capability {
        Capability::Areas
    }

    fn unit_type(&self) -> &DeclaredName {
        &self.area
    }

    fn fan(&self) -> Fan {
        Fan::ONE
    }
}

impl DataSection for AreaDeliveryData {
    const TABLE: DataTable = DeliverySections::TABLE;
}

impl Listed for AreaDeliveryData {
    type Set = DeliverySections;

    const KEY: &'static str = "area";

    const STATUS: Status = Status::Runs(ApiVersion::FIRST);

    fn into_listed(self) -> Arc<dyn DeliverySection> {
        Arc::new(self)
    }

    fn api(_: &mut ApiBuilder<'_>) {}
}
