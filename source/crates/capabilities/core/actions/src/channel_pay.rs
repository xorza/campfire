use core::num::NonZeroU32;

use campfire_values::{ResourceId, TrackId};

/// What each tick of an action's channel pays its unit's owner: an amount of `resource`, by the
/// unit's level on `track`, from level 1, the last amount for each level past them; with no track,
/// its one amount (04-capabilities/actions.md, A channel with no end).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelPay {
    pub resource: ResourceId,
    pub track: Option<TrackId>,
    amounts: Box<[u32]>,
}

impl ChannelPay {
    /// The pay of `amounts`, each above 0, by level on `track`; none for no amount, an amount of
    /// 0, or more than one with no track.
    pub(crate) fn new(
        resource: ResourceId,
        track: Option<TrackId>,
        amounts: &[u32],
    ) -> Option<Self> {
        let fits = !amounts.is_empty()
            && amounts.iter().all(|&amount| amount > 0)
            && (track.is_some() || amounts.len() == 1);
        fits.then(|| Self {
            resource,
            track,
            amounts: amounts.into(),
        })
    }

    /// The amount a tick pays of a unit at `level` on its track.
    pub fn amount(&self, level: NonZeroU32) -> u32 {
        let last = self.amounts.len() - 1;
        let at = usize::try_from(level.get() - 1).map_or(last, |at| at.min(last));
        self.amounts[at]
    }
}

#[cfg(test)]
mod tests {
    use campfire_values::NameId;

    use super::*;

    /// Levels 1 and 2 pay their own amounts; level 5, past the list, its last; with no track one
    /// amount, and an empty list, a 0 or two amounts with no track pay nothing.
    #[test]
    fn a_pay_takes_its_level_s_amount_and_the_last_past_the_list() {
        let (resource, track) = (ResourceId::at(0), TrackId::at(1));
        let level = |level| NonZeroU32::new(level).unwrap();
        let pay = ChannelPay::new(resource, Some(track), &[5, 8, 10]).unwrap();
        assert_eq!(pay.amount(level(1)), 5);
        assert_eq!(pay.amount(level(2)), 8);
        assert_eq!(pay.amount(level(5)), 10);
        let flat = ChannelPay::new(resource, None, &[7]).unwrap();
        assert_eq!(flat.amount(level(3)), 7);
        assert_eq!(ChannelPay::new(resource, Some(track), &[]), None);
        assert_eq!(ChannelPay::new(resource, Some(track), &[4, 0]), None);
        assert_eq!(ChannelPay::new(resource, None, &[4, 5]), None);
    }
}
