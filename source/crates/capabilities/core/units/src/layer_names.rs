use campfire_declare::Fact;
use campfire_values::Names;

use crate::layer::Layer;

/// The names of the layers bodies move on, the mode's `[navigation] layers`, which the module
/// that reads that section makes: what a unit type's `collision` names its layer by.
#[derive(Debug, Clone, Default)]
pub struct LayerNames(pub Names<Layer>);

impl Fact for LayerNames {}
