use crate::zero_hour::object_ini::{DefaultLook, WeaponBones};

/// What a model's default look reads of it: its sub-objects, in the HLOD's order, each with its
/// full name, its pivot, whether the model has a node of it, and whether its mesh is hidden by
/// its flag; and its pivots' names and parents. Names are lowercase.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ModelParts {
    pub(crate) sub_objects: Vec<Part>,
    pub(crate) pivots: Vec<PivotName>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Part {
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

impl ModelParts {
    /// The names of the drawn sub-objects `look` leaves hidden, as `W3DModelDraw` sets a state: those
    /// hidden by their flag; then each weapon slot's muzzle flashes; then each hide or show in
    /// order, also over the sub-objects on the strict child pivots of its own.
    pub(crate) fn hidden(&self, look: &DefaultLook) -> Vec<String> {
        let mut hidden: Vec<bool> = self.sub_objects.iter().map(|part| part.hidden).collect();
        for weapon in &look.weapons {
            for flash in self.muzzle_flashes(weapon) {
                if let Some(at) = self.sub_objects.iter().position(|part| part.pivot == flash) {
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
            for (other, part) in self.sub_objects.iter().enumerate() {
                if self.below(part.pivot, pivot) {
                    hidden[other] = change.hide;
                }
            }
        }
        self.sub_objects
            .iter()
            .zip(hidden)
            .filter(|(part, hidden)| part.drawn && *hidden)
            .map(|(part, _)| part.name.clone())
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
            let numbered = |bone: &Option<String>| {
                bone.as_ref()
                    .and_then(|bone| self.pivot(&format!("{bone}{number:02}")))
            };
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

    fn pivot(&self, name: &str) -> Option<usize> {
        self.pivots
            .iter()
            .position(|pivot| pivot.name.eq_ignore_ascii_case(name))
    }

    /// The sub-object of `name`, as `Get_Sub_Object_By_Name` finds it: by its full name, else by
    /// the part of it after its first `.`, ignoring case.
    fn sub_object(&self, name: &str) -> Option<usize> {
        let parts = &self.sub_objects;
        parts
            .iter()
            .position(|part| part.name.eq_ignore_ascii_case(name))
            .or_else(|| {
                parts.iter().position(|part| {
                    let suffix = part
                        .name
                        .split_once('.')
                        .map_or(part.name.as_str(), |(_, suffix)| suffix);
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
mod tests {
    use super::*;
    use crate::zero_hour::object_ini::HideShow;

    /// A tank's pivots, each with its parent: the root; the turret; two numbered flashes, a
    /// barrel and a bare flash on the turret; and on the root, a bare flash and a slot's
    /// numbered fire bones beside a bare flash of its own.
    fn tank() -> ModelParts {
        let pivots = [
            ("roottransform", None),
            ("turret", Some(0)),
            ("muzzlefx01", Some(1)),
            ("muzzlefx02", Some(1)),
            ("barrel", Some(1)),
            ("solo", Some(0)),
            ("flash", Some(0)),
            ("fire01", Some(0)),
            ("fire02", Some(0)),
        ]
        .map(|(name, parent)| PivotName {
            name: name.to_owned(),
            parent,
        });
        let part = |name: &str, pivot, drawn, hidden| Part {
            name: name.to_owned(),
            pivot,
            drawn,
            hidden,
        };
        ModelParts {
            sub_objects: vec![
                part("tank.hull", 0, true, false),
                part("tank.turret", 1, true, false),
                part("tank.flash1", 2, true, false),
                part("tank.flash2a", 3, true, false),
                part("tank.flash2b", 3, true, false),
                part("tank.barrel", 4, true, false),
                part("tank.solo", 5, true, false),
                part("tank.decal", 0, true, true),
                part("tank.pickbox", 0, false, true),
                part("tank.flashb", 6, true, false),
            ],
            pivots: pivots.to_vec(),
        }
    }

    fn bones(fire_fx: Option<&str>, muzzle_flash: &str) -> WeaponBones {
        WeaponBones {
            fire_fx: fire_fx.map(str::to_owned),
            muzzle_flash: Some(muzzle_flash.to_owned()),
            ..WeaponBones::default()
        }
    }

    #[test]
    fn a_default_look_hides_flagged_parts_muzzle_flashes_and_its_hidden_sub_objects() {
        let parts = tank();
        // Slot 1 numbers `MuzzleFX` 01 and 02 and stops at 03: the first part on each bone. Slot
        // 2 finds `Fire01` and `Fire02` but no numbered flash, so not its bare `Flash` either.
        // Slot 3 numbers nothing, so its bare `Solo`. The decal is hidden by its flag; the box,
        // which has no node, is never named.
        let flashes = DefaultLook {
            hide_show: Vec::new(),
            weapons: [
                bones(None, "MuzzleFX"),
                bones(Some("Fire"), "Flash"),
                bones(None, "SOLO"),
            ],
        };
        assert_eq!(
            parts.hidden(&flashes),
            ["tank.flash1", "tank.flash2a", "tank.solo", "tank.decal"]
        );
        // Each change in order, by its full name or its name after the first `.`: the turret
        // hides the parts on the pivots below its own, the barrel is shown again, and a name of
        // no part is skipped.
        let change = |name: &str, hide| HideShow {
            name: name.to_owned(),
            hide,
        };
        let look = |hide_show| DefaultLook {
            hide_show,
            ..DefaultLook::default()
        };
        let turret = look(vec![
            change("TURRET", true),
            change("barrel", false),
            change("missing", true),
        ]);
        assert_eq!(
            parts.hidden(&turret),
            [
                "tank.turret",
                "tank.flash1",
                "tank.flash2a",
                "tank.flash2b",
                "tank.decal"
            ]
        );
        // A part on the root spreads to every part off the root, as `doHideShowBoneSubObjs`
        // walks each part's bone up to the root: the hull hides all, then the decal shows all
        // but the hull.
        let root = look(vec![change("tank.hull", true)]);
        assert_eq!(parts.hidden(&root).len(), 9);
        let root = look(vec![change("tank.hull", true), change("Decal", false)]);
        assert_eq!(parts.hidden(&root), ["tank.hull"]);
        assert_eq!(parts.hidden(&DefaultLook::default()), ["tank.decal"]);
    }
}
