/// A link's condition in the Gilbert–Elliott model that decides each packet's loss (see
/// [`LinkModel`](crate::link_model::LinkModel)).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum LossState {
    /// The link is healthy. Usually means packets are not be getting lost or
    /// delayed.
    #[default]
    Good,

    /// The link is degraded. Usually entails packets getting lost or delayed.
    Bad,
}
