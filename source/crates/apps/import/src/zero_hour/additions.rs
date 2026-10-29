use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::collections::btree_map::{Iter, IterMut};

use campfire_airfields::AirfieldData;
use campfire_combat::BarrelsData;
use campfire_common::DeclaredName;
use campfire_interaction::{EnterData, RidesData};
use campfire_production::{DropOffData, NodeData, ProductionData, SellData, SpawnsData};
use campfire_progression::{ExperienceData, TypeTracksData};
use campfire_stats::UpgradesData;

use crate::zero_hour::area_write::AreaWrite;
use crate::zero_hour::model_bones::ModelBones;
use crate::zero_hour::type_name::TypeName;

/// What the import's stages give each unit type beside the fields its object gives, by the
/// type's name: each stage sets its own as it runs, a later stage reads an earlier one's, and the
/// units' file writes each type's with its own fields (09-zero-hour.md, The stages give a type's
/// sections to one store).
#[derive(Debug, Default)]
pub(crate) struct Additions {
    types: BTreeMap<TypeName, TypeAdditions>,
}

/// What the stages give one unit type: the actions of its slot kinds `weapon`, in its starting
/// weapon set's order, and `command`, in its command set's order, its gather last; its
/// `production` section when it trains; its `sell` when its command set sells it; its `node` when
/// it is a dock and its `drop_off` when it is a supply center; and its `upgrades`
/// (09-zero-hour.md, Every command set's trains and builds are imported; 09-zero-hour.md, A weapon
/// converts when it hits one target at once; 09-zero-hour.md, A box is money;
/// 04-capabilities/stats.md, A unit type takes modifiers by upgrades).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct TypeAdditions {
    pub(crate) weapons: Vec<DeclaredName>,
    /// The weapons of each of its weapon sets' slots, `PRIMARY` first, which its turret carries
    /// by.
    pub(crate) weapon_slots: Vec<Vec<DeclaredName>>,
    pub(crate) commands: Vec<DeclaredName>,
    pub(crate) production: Option<ProductionData>,
    pub(crate) sell: Option<SellData>,
    pub(crate) airfield: Option<AirfieldData>,
    pub(crate) node: Option<NodeData>,
    pub(crate) drop_off: Option<DropOffData>,
    pub(crate) spawns: Option<SpawnsData>,
    pub(crate) upgrades: Option<UpgradesData>,
    pub(crate) barrels: Option<BarrelsData>,
    /// The bones its turret reads of its model.
    pub(crate) bones: Option<ModelBones>,
    /// The fire points of a garrison whose model names one, on the ground in its object's frame,
    /// the game's axes.
    pub(crate) fire_points: Vec<[f32; 2]>,
    /// Its track of veterancy and what its death gives (09-zero-hour.md, Veterancy is
    /// progression's data).
    pub(crate) tracks: Option<TypeTracksData>,
    pub(crate) experience: Option<ExperienceData>,
    /// The area of the marker a power's cast lands (09-zero-hour.md, The import converts a
    /// marker's burst).
    pub(crate) area: Option<AreaWrite>,
    /// The `enter` of a container, and the `rides` of a rider, which takes a `uses` of the slot
    /// kind of its entry (04-capabilities/interaction.md, A container says who rides).
    pub(crate) enter: Option<EnterData>,
    pub(crate) rides: Option<RidesData>,
}

impl Additions {
    /// What the stages gave the type of `name`, if any.
    pub(crate) fn get<Q: Ord + ?Sized>(&self, name: &Q) -> Option<&TypeAdditions>
    where
        TypeName: Borrow<Q>,
    {
        self.types.get(name)
    }

    /// The actions of the slot kind `command` of the type of `name`, in its command set's order.
    pub(crate) fn commands<Q: Ord + ?Sized>(&self, name: &Q) -> &[DeclaredName]
    where
        TypeName: Borrow<Q>,
    {
        self.get(name).map_or(&[], |added| &added.commands)
    }

    /// What the stages give the type of `name`, none yet when no stage gave it any.
    pub(crate) fn type_mut(&mut self, name: &TypeName) -> &mut TypeAdditions {
        self.types.entry(name.clone()).or_default()
    }

    /// Each type the stages gave anything, in the order of their names, to give it more.
    pub(crate) fn iter_mut(&mut self) -> IterMut<'_, TypeName, TypeAdditions> {
        self.types.iter_mut()
    }

    /// Each type the stages gave anything, in the order of their names.
    pub(crate) fn iter(&self) -> Iter<'_, TypeName, TypeAdditions> {
        self.types.iter()
    }
}

impl<'a> IntoIterator for &'a Additions {
    type Item = (&'a TypeName, &'a TypeAdditions);
    type IntoIter = Iter<'a, TypeName, TypeAdditions>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
