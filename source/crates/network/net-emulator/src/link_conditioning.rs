use crate::link_model::LinkModel;

/// The network a `LinkConditioner` puts in front of a
/// real IO: the conditions it applies, and the seed its draws follow, so that two runs of one
/// model on the same packet sequence condition it identically.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LinkConditioning {
    pub model: LinkModel,
    pub seed: u64,
}

impl From<LinkModel> for LinkConditioning {
    fn from(model: LinkModel) -> Self {
        Self { model, seed: 0 }
    }
}
