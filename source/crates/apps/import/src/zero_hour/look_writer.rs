use campfire_client_files::{ClientModel, ClientUnit, ClientUnits, LookVariant, ModelScale};
use campfire_codec::Toml;

use crate::zero_hour::error::ZeroHourError;
use crate::zero_hour::format::ini::object_ini::{
    DrawKind, DrawnState, ObjectIni, PlacedAs, StateLook,
};
use crate::zero_hour::format::look_conditions::LookConditions;
use crate::zero_hour::game_path;
use crate::zero_hour::import_names::ImportNames;
use crate::zero_hour::map_import;
use crate::zero_hour::model_writer::{ModelWriter, WrittenModel};

/// What writes the client's looks of the objects, `client/units.toml`, each model it draws
/// converted once through `models`.
#[derive(Debug)]
pub(crate) struct LookWriter<'m, 'i, 'w> {
    pub(crate) models: &'m mut ModelWriter<'i, 'w>,
}

/// The models a look draws on a map of some conditions, and whether the package lacks a model
/// it names.
#[derive(Debug)]
struct LookModels {
    models: Vec<ClientModel>,
    skipped: bool,
}

impl LookWriter<'_, '_, '_> {
    /// Writes `client/units.toml` of `ini`'s objects, and gives it: each object's look, its models,
    /// each at its object's asset scale, with the nodes its default look hides, a look of no models
    /// for an object whose draw modules name none, as it draws nothing, and no look for one whose
    /// models the package lacks, which the client shows as the engine's shape; and the look of each
    /// object that maps place as client objects, as the client's tree or prop buffer draws it
    /// (12-zero-hour.md, The multiplayer rule for client objects): a tree's models of its tree draw
    /// modules, a prop's of its first draw module, when the `W3DModelDraw` family's, each created
    /// by its name, so only the meshes hidden by their own flag are hidden.
    pub(crate) fn units(
        &mut self,
        ini: &ObjectIni,
        names: &mut ImportNames,
    ) -> Result<ClientUnits, ZeroHourError> {
        let mut units = ClientUnits::default();
        let shown = StateLook::default();
        for object in ini.objects() {
            let scale = object.scale();
            let draws = object.draws();
            // A unit draws each module's best state, as its drawable does.
            let unit = |conditions| draws.iter().map(move |draw| draw.drawn(conditions));
            let Some(mut unit_look) = self.look(unit, scale)? else {
                continue;
            };
            // A preview of its build stands at the angle the original's cursor shows it at.
            unit_look.placement_angle = object.build().placement_angle;
            let unit_type = names.take(object)?;
            // A tree draws its module's model whatever the map; a prop the model of its first
            // module's best state, all of it shown, as `W3DTerrainVisual::addProp` draws it.
            let unhidden = |model| DrawnState {
                model,
                look: &shown,
            };
            let drawn = |conditions| match object.placed_as() {
                PlacedAs::Bridge | PlacedAs::Unit => Vec::new(),
                PlacedAs::Tree => draws
                    .iter()
                    .filter(|draw| draw.kind == DrawKind::Tree)
                    .map(|draw| unhidden(draw.model.as_deref()))
                    .collect(),
                PlacedAs::Prop => draws
                    .first()
                    .filter(|draw| draw.kind == DrawKind::Family)
                    .map(|draw| unhidden(draw.drawn(conditions).model))
                    .into_iter()
                    .collect(),
            };
            if let Some(object_look) = self.look(drawn, scale)?
                && !object_look.models.is_empty()
            {
                units.objects.insert(unit_type.declared(), object_look);
            }
            units.units.insert(unit_type.declared(), unit_look);
        }
        let text = Toml::write(&units);
        self.models.write(ClientUnits::path(), text.as_bytes())?;
        Ok(units)
    }

    /// The look of what `drawn` draws on a map of each conditions, at `scale`: its models on a
    /// map of none, and, when a night map, a snowy map or both draw other models, a variant for
    /// each of the three, so the client's best match finds each one's own. A look that draws
    /// nothing is empty when no model it names draws anything, and none when the package lacks
    /// one, so the client shows its loss.
    fn look<'d, I>(
        &mut self,
        drawn: impl Fn(LookConditions) -> I,
        scale: ModelScale,
    ) -> Result<Option<ClientUnit>, ZeroHourError>
    where
        I: IntoIterator<Item = DrawnState<'d>>,
    {
        let own = self.models(drawn(LookConditions::default()), scale)?;
        let mut skipped = own.skipped;
        let mut variants = Vec::new();
        for conditions in LookConditions::VARIANTS {
            let looked = self.models(drawn(conditions), scale)?;
            skipped |= looked.skipped;
            variants.push(LookVariant {
                conditions: map_import::condition_names(conditions),
                models: looked.models,
            });
        }
        if variants.iter().all(|variant| variant.models == own.models) {
            variants.clear();
        }
        if skipped && own.models.is_empty() && variants.is_empty() {
            return Ok(None);
        }
        Ok(Some(ClientUnit {
            models: own.models,
            variants,
            placement_angle: None,
        }))
    }

    /// The models of `drawn`, each with the look whose hidden nodes it hides, at `scale`, of those
    /// that draw one the package holds, and whether the package lacks one it names.
    fn models<'d>(
        &mut self,
        drawn: impl IntoIterator<Item = DrawnState<'d>>,
        scale: ModelScale,
    ) -> Result<LookModels, ZeroHourError> {
        let mut looked = LookModels {
            models: Vec::new(),
            skipped: false,
        };
        for DrawnState { model, look } in drawn {
            let Some(model) = model else {
                continue;
            };
            let name = model.to_ascii_lowercase();
            match self.models.model(&name)? {
                WrittenModel::Parts(parts) => looked.models.push(ClientModel {
                    model: game_path::model(&name)?,
                    scale,
                    hide: parts.hidden(look),
                }),
                WrittenModel::Nothing => {}
                WrittenModel::Skipped => looked.skipped = true,
            }
        }
        Ok(looked)
    }
}
