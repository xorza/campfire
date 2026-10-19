use crate::confirmed_history_fns::ConfirmedHistoryFns;
use crate::frame_history_fns::FrameHistoryFns;
use crate::frame_interpolation_history::FrameInterpolationHistory;
use crate::frame_interpolation_rules::install_frame_history;
use crate::frame_source::FrameSource;
use crate::history_component::HistoryComponent;
use crate::interpolation_fns::InterpolationPipeline;
use bevy_ecs::component::ComponentId;
use bevy_ecs::world::World;
use campfire_net_replication::ComponentKind;
use campfire_net_replication::ConfirmedHistory;

/// One component owned by an interpolation rule.
///
/// The history installers live on the member instead of in a separate global provider catalog.
/// Each rule therefore carries the history behavior needed to initialize and maintain its
/// members as well as its apply behavior.
#[derive(Debug, Clone)]
pub(crate) struct InterpolationRuleComponent {
    /// Type identity used to prevent rules from claiming the same member.
    pub(crate) kind: ComponentKind,
    /// Bevy component IDs whose presence can make this member available.
    pub(crate) live_component_id: ComponentId,
    pub(crate) confirmed_history_component_id: ComponentId,
    pub(crate) frame_history_component_id: ComponentId,
    /// History behavior owned by this rule, independently of the IDs above.
    pub(crate) confirmed: Option<ConfirmedHistoryFns>,
    pub(crate) frame: Option<FrameHistoryFns>,
    /// Where frame interpolation reads the member's samples in place of its frame history.
    pub(crate) frame_source: Option<FrameSource>,
}

/// What a rule member owns of its component's histories, which [`InterpolationRuleComponent::of`]
/// builds the member from.
#[derive(Debug, Clone)]
pub(crate) struct MemberSpec {
    /// How its confirmed history is written, when the rule owns it.
    pub(crate) confirmed: Option<ConfirmedHistoryFns>,
    pub(crate) pipeline: InterpolationPipeline,
    /// Where frame interpolation reads its samples, when another plugin keeps them.
    pub(crate) frame_source: Option<FrameSource>,
}

impl InterpolationRuleComponent {
    /// The member of `C` that `spec` describes, its ids registered in `world`.
    pub(crate) fn of<C>(world: &mut World, spec: MemberSpec) -> Self
    where
        C: HistoryComponent,
    {
        let MemberSpec {
            confirmed,
            pipeline,
            frame_source,
        } = spec;
        Self {
            kind: ComponentKind::of::<C>(),
            // Every member needs all representation IDs for archetype matching,
            // including disabled rules that own no history or apply callbacks.
            live_component_id: world.register_component::<C>(),
            confirmed_history_component_id: world.register_component::<ConfirmedHistory<C>>(),
            frame_history_component_id: world.register_component::<FrameInterpolationHistory<C>>(),
            confirmed,
            frame: pipeline.owns_frame_history().then_some(FrameHistoryFns {
                install: install_frame_history::<C>,
            }),
            frame_source,
        }
    }
}
