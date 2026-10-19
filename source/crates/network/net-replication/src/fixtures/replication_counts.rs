use campfire_net_channels::ChannelTypeId;
use campfire_net_channels::internals::ChannelCounts;
use campfire_net_stepper::Stepper;
use campfire_net_transport::Transport;

use crate::replicon_mutations_channel::RepliconMutationsChannel;
use crate::replicon_updates_channel::RepliconUpdatesChannel;

/// The counters of replicon's two replication channels over every transport in a stepper's apps,
/// summed.
pub fn summed_replication_counts<S>(stepper: &mut Stepper<S>) -> ChannelCounts {
    let channels = [
        ChannelTypeId::of::<RepliconUpdatesChannel>(),
        ChannelTypeId::of::<RepliconMutationsChannel>(),
    ];
    let mut counts = ChannelCounts::default();
    for app in stepper.apps_mut() {
        let world = app.world_mut();
        for transport in world.query::<&Transport>().iter(world) {
            for type_id in channels {
                counts = counts + transport.channel_counts(type_id);
            }
        }
    }
    counts
}
