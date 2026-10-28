use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_math::Num;
use campfire_script::{ApiVersion, DataSection, DataTable, Status};
use campfire_values::{FilterData, Share, TagFilterData};
use serde::{Deserialize, Serialize};

/// A unit type's `enter` section, which makes its units containers that others ride in
/// (04-capabilities/interaction.md, A container says who rides): its entry, an action of its
/// package of the `enter` kind, which a rider's use slot runs at it; how many it holds, of each
/// rider's `size` or one each; the riders it takes, those any of its filters selects by their
/// relation to it and their tags, all of them when absent; the milliseconds between two exits; the
/// share of each rider's life most its death deals it; the share of its own life at or below which
/// it takes none and lets every rider out, none when absent; whether one of no player takes its
/// first rider's; the riders it spawns with; where its riders fire from, none when absent; whether
/// its riders see from inside; the modifier of its package each rider holds inside
/// (04-capabilities/combat.md, A container may let its riders fire; 04-capabilities/interaction.md,
/// A garrison's riders see from it; 04-capabilities/combat.md, A container's modifier holds on its
/// riders); the tags it holds while it takes riders, every time when absent
/// (exits-docks-and-selection.md, A bunker is an entry a tag opens); and the share of its life
/// above which a rider leaves to fight as it is ordered to attack, none for riders that stay
/// (exits-docks-and-selection.md, A transport may assault).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnterData {
    pub action: DeclaredName,
    pub capacity: u32,
    #[serde(default)]
    pub counts: RiderCount,
    #[serde(default = "every_rider")]
    pub riders: Vec<FilterData>,
    #[serde(default)]
    pub exit_ms: u32,
    #[serde(default = "no_share")]
    pub damage: Share,
    #[serde(default = "no_share")]
    pub full: Share,
    #[serde(default)]
    pub capture: bool,
    #[serde(default)]
    pub payload: Vec<PayloadData>,
    #[serde(default)]
    pub fire: FireData,
    #[serde(default)]
    pub sight: bool,
    pub modifier: Option<DeclaredName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<TagFilterData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assault: Option<Share>,
}

/// Where a container's riders fire from: nowhere; its own place, those of the tags of a filter;
/// or its fire points, `[x, z]` meters of its frame, each rider the free one nearest its target.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FireData {
    #[default]
    None,
    Center(TagFilterData),
    Points(Vec<[Num; 2]>),
}

/// What a rider takes of a container's capacity: its type's `size`, as a transport counts
/// `TransportSlotCount`, or one, as a garrison counts each rider.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiderCount {
    #[default]
    Size,
    One,
}

/// Riders a container spawns with: `count` of the unit type `unit` of its package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PayloadData {
    pub unit: DeclaredName,
    pub count: u32,
}

fn every_rider() -> Vec<FilterData> {
    vec![FilterData::all()]
}

const fn no_share() -> Share {
    Share::NONE
}

impl DataSection for EnterData {
    const TABLE: DataTable = DataTable::new("A unit type's `enter`");
}

impl Section for EnterData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "enter";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
