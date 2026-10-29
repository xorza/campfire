use campfire_client_files::{ClientModel, ClientUnit, ClipRounds, FireBone, LookDraw};
use campfire_common::DeclaredName;
use campfire_values::PackagePath;

/// What of a portable structure's model, which its carrier's draws hold at a bone, waits for the
/// weapon slots the structure's weapons take among its carrier's: the barrels and the clips' rounds
/// of the structure's own slots, and the first of its carrier's slots they take, once the combat
/// import arms the carrier with them (riders-and-upgrades.md, A carried model's barrels are its
/// carrier's).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CarriedLook {
    structure: DeclaredName,
    model: PackagePath,
    fire_bones: Vec<FireBone>,
    rounds: Vec<ClipRounds>,
    first_slot: Option<u8>,
}

impl CarriedLook {
    /// The look of the structure `structure`, whose model `model`, drawn at a bone, holds
    /// `fire_bones` and shows `rounds`, each of the structure's own weapon slots.
    pub(crate) const fn new(
        structure: DeclaredName,
        model: PackagePath,
        fire_bones: Vec<FireBone>,
        rounds: Vec<ClipRounds>,
    ) -> Self {
        Self {
            structure,
            model,
            fire_bones,
            rounds,
            first_slot: None,
        }
    }

    /// Takes `first_slot`, the first of its carrier's weapon slots that the weapons of the
    /// structure `structure` take, when it is that structure's.
    pub(crate) fn arm(&mut self, structure: &DeclaredName, first_slot: usize) {
        if self.structure == *structure {
            let first_slot = u8::try_from(first_slot).expect("a look's weapon slots fit a u8");
            self.first_slot = Some(first_slot);
        }
    }

    /// Gives each state of `look`'s draws and its variations' that holds its model at a bone the
    /// barrels and the rounds of its carrier's slots, of that model's place; none for a structure
    /// whose weapons arm no slot.
    pub(crate) fn carry(&self, look: &mut ClientUnit) {
        let Some(first_slot) = self.first_slot else {
            return;
        };
        let variations = look.variations.iter_mut();
        let draws = look
            .draws
            .iter_mut()
            .chain(variations.flat_map(|variation| &mut variation.draws));
        for draw in draws {
            self.carry_in(draw, first_slot);
        }
    }

    fn carry_in(&self, draw: &mut LookDraw, first_slot: u8) {
        for state in draw.states_mut() {
            let carried = state
                .models
                .iter()
                .position(|model| model.bone.is_some() && model.model == self.model);
            let Some(place) = carried else {
                continue;
            };
            let model: &mut ClientModel = &mut state.models[place];
            model.rounds = (self.rounds.iter())
                .map(|clip| ClipRounds {
                    slot: first_slot + clip.slot,
                    rounds: clip.rounds.clone(),
                })
                .collect();
            let place = u8::try_from(place).expect("a state's models fit a u8");
            state
                .fire_bones
                .extend(self.fire_bones.iter().map(|bone| FireBone {
                    slot: first_slot + bone.slot,
                    model: place,
                    barrels: bone.barrels.clone(),
                }));
        }
    }
}
