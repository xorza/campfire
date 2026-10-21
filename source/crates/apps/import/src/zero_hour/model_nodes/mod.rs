use crate::zero_hour::format::ini::object_ini::{StateLook, WeaponBones};

/// What a model's default look reads of it: its sub-objects, in the HLOD's order, each with its
/// full name, its pivot, whether the model has a node of it, and whether its mesh is hidden by
/// its flag; and its pivots' names and parents. Names are lowercase.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ModelNodes {
    pub(crate) sub_objects: Vec<LookSubObject>,
    pub(crate) pivots: Vec<PivotName>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LookSubObject {
    pub(crate) name: String,
    pub(crate) pivot: usize,
    /// False for a collision box, which the game never draws.
    pub(crate) drawn: bool,
    pub(crate) hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PivotName {
    pub(crate) name: String,
    pub(crate) parent: Option<usize>,
}

impl ModelNodes {
    /// The names of the drawn sub-objects `look` leaves hidden, as `W3DModelDraw` sets a state:
    /// those hidden by their flag; then each weapon slot's muzzle flashes; then each hide or show
    /// in order, also over the sub-objects on the strict child pivots of its own.
    pub(crate) fn hidden(&self, look: &StateLook) -> Vec<String> {
        let mut hidden: Vec<bool> = self
            .sub_objects
            .iter()
            .map(|sub_object| sub_object.hidden)
            .collect();
        for weapon in &look.weapons {
            for flash in self.muzzle_flashes(weapon) {
                if let Some(at) = self
                    .sub_objects
                    .iter()
                    .position(|sub_object| sub_object.pivot == flash)
                {
                    hidden[at] = true;
                }
            }
        }
        for change in &look.hide_show {
            let Some(at) = self.sub_object(&change.name) else {
                continue;
            };
            hidden[at] = change.hide;
            let pivot = self.sub_objects[at].pivot;
            for (other, sub_object) in self.sub_objects.iter().enumerate() {
                if self.below(sub_object.pivot, pivot) {
                    hidden[other] = change.hide;
                }
            }
        }
        self.sub_objects
            .iter()
            .zip(hidden)
            .filter(|(sub_object, hidden)| sub_object.drawn && *hidden)
            .map(|(sub_object, _)| sub_object.name.clone())
            .collect()
    }

    /// The pivots of a weapon slot's muzzle flashes, as `validateWeaponBarrelInfo` finds them:
    /// for each number from 01 to 99, while any of the slot's bones has that number, the flash's;
    /// else, if no number found one, the flash's bare name.
    fn muzzle_flashes(&self, weapon: &WeaponBones) -> Vec<usize> {
        let bones = [
            &weapon.fire_fx,
            &weapon.recoil,
            &weapon.muzzle_flash,
            &weapon.launch,
        ];
        let mut flashes = Vec::new();
        let mut barrels = 0;
        for number in 1..=99 {
            let numbered =
                |bone: &Option<String>| self.pivot(&format!("{}{number:02}", bone.as_ref()?));
            if bones.iter().all(|bone| numbered(bone).is_none()) {
                break;
            }
            barrels += 1;
            flashes.extend(numbered(&weapon.muzzle_flash));
        }
        if barrels == 0 {
            flashes.extend(
                weapon
                    .muzzle_flash
                    .as_ref()
                    .and_then(|flash| self.pivot(flash)),
            );
        }
        flashes
    }

    /// The name of its pivot `name`, of either case, as its node is named.
    pub(crate) fn pivot_name(&self, name: &str) -> Option<&str> {
        Some(self.pivots[self.pivot(name)?].name.as_str())
    }

    fn pivot(&self, name: &str) -> Option<usize> {
        self.pivots
            .iter()
            .position(|pivot| pivot.name.eq_ignore_ascii_case(name))
    }

    /// The sub-object of `name`, as `Get_Sub_Object_By_Name` finds it: by its full name, else by
    /// the part of it after its first `.`, ignoring case.
    fn sub_object(&self, name: &str) -> Option<usize> {
        let nodes = &self.sub_objects;
        nodes
            .iter()
            .position(|sub_object| sub_object.name.eq_ignore_ascii_case(name))
            .or_else(|| {
                nodes.iter().position(|sub_object| {
                    let suffix = sub_object
                        .name
                        .split_once('.')
                        .map_or(sub_object.name.as_str(), |(_, suffix)| suffix);
                    suffix.eq_ignore_ascii_case(name)
                })
            })
    }

    /// Whether `pivot` lies strictly below `ancestor`; pivot 0, the root, lies below none.
    fn below(&self, pivot: usize, ancestor: usize) -> bool {
        let mut at = pivot;
        while at != 0 {
            match self.pivots.get(at).and_then(|pivot| pivot.parent) {
                Some(parent) => at = parent,
                None => return false,
            }
            if at == ancestor {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests;
