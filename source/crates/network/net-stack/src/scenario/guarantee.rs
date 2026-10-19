/// What a channel promises about the messages it delivers, checked against what was sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Guarantee {
    /// Each message at most once (unreliable).
    AtMostOnce,
    /// Never a message older than one already delivered (sequenced unreliable).
    Increasing,
    /// Each message exactly once (unordered reliable).
    ExactlyOnce,
    /// Never an older message, and the newest one arrives (sequenced reliable).
    IncreasingWithNewest,
    /// Each message exactly once, in the order sent (ordered reliable).
    ExactlyOnceInOrder,
}

impl Guarantee {
    /// Checks `received` against `sent`, which is `0..sent.len()` in order.
    pub(crate) fn check(self, sent: &[u32], received: &[u32]) -> Result<(), String> {
        if let Some(stray) = received
            .iter()
            .find(|seq| usize::try_from(**seq).unwrap() >= sent.len())
        {
            return Err(format!("{stray} was never sent"));
        }
        let increasing = || received.windows(2).all(|pair| pair[0] < pair[1]);
        let mut sorted = received.to_vec();
        sorted.sort_unstable();
        let once = sorted.windows(2).all(|pair| pair[0] != pair[1]);
        match self {
            Self::AtMostOnce if !once => Err("a message arrived twice".into()),
            Self::Increasing if !increasing() => Err("an older message arrived".into()),
            Self::ExactlyOnce if sorted != sent => Err(format!(
                "{} of {} messages arrived, or some twice",
                received.len(),
                sent.len()
            )),
            Self::IncreasingWithNewest if !increasing() => Err("an older message arrived".into()),
            Self::IncreasingWithNewest if received.last() != sent.last() => Err(format!(
                "the newest message {:?} did not arrive; the last was {:?}",
                sent.last(),
                received.last()
            )),
            Self::ExactlyOnceInOrder if received != sent => Err(format!(
                "{} of {} messages arrived in order",
                received
                    .iter()
                    .zip(sent)
                    .take_while(|(received, sent)| received == sent)
                    .count(),
                sent.len()
            )),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_guarantee_accepts_and_rejects_exactly() {
        let sent = [0, 1, 2, 3];
        let cases: [(Guarantee, &[u32], bool); 14] = [
            (Guarantee::AtMostOnce, &[3, 0], true),
            (Guarantee::AtMostOnce, &[1, 1], false),
            (Guarantee::AtMostOnce, &[4], false),
            (Guarantee::Increasing, &[0, 2, 3], true),
            (Guarantee::Increasing, &[2, 1], false),
            (Guarantee::ExactlyOnce, &[2, 0, 3, 1], true),
            (Guarantee::ExactlyOnce, &[0, 1, 2], false),
            (Guarantee::ExactlyOnce, &[0, 1, 2, 2], false),
            (Guarantee::IncreasingWithNewest, &[1, 3], true),
            (Guarantee::IncreasingWithNewest, &[1, 2], false),
            (Guarantee::IncreasingWithNewest, &[3, 3], false),
            (Guarantee::ExactlyOnceInOrder, &[0, 1, 2, 3], true),
            (Guarantee::ExactlyOnceInOrder, &[0, 2, 1, 3], false),
            (Guarantee::ExactlyOnceInOrder, &[0, 1, 2], false),
        ];
        for (guarantee, received, ok) in cases {
            assert_eq!(
                guarantee.check(&sent, received).is_ok(),
                ok,
                "{guarantee:?} on {received:?}"
            );
        }
    }
}
