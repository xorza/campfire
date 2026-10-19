use campfire_net_channels::ChannelKind;
use campfire_net_channels::internals::channel_counts::ChannelCounts;
use campfire_net_stepper::Stepper;
use campfire_net_transport::Transport;

use crate::replicon_mutations_channel::RepliconMutationsChannel;
use crate::replicon_updates_channel::RepliconUpdatesChannel;

/// The counters of replicon's two replication channels over every transport in a stepper's apps,
/// summed.
pub fn read<S>(stepper: &mut Stepper<S>) -> ChannelCounts {
    let channels = [
        ChannelKind::of::<RepliconUpdatesChannel>(),
        ChannelKind::of::<RepliconMutationsChannel>(),
    ];
    let mut counts = ChannelCounts::default();
    for app in stepper.apps_mut() {
        let world = app.world_mut();
        for transport in world.query::<&Transport>().iter(world) {
            for kind in channels {
                counts = counts + transport.channel_counts(kind);
            }
        }
    }
    counts
}
