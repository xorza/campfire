use super::*;

/// The checkpoint record of the segment after `log`'s last, at its next tick, carrying its own
/// state, with a state hash and a snapshot fingerprint of `byte`.
fn checkpoint(log: &SessionLog, byte: u8) -> Checkpoint {
    Checkpoint {
        segment: log.segment() + 1,
        tick: log.next_tick(),
        state_hash: StateHash::new([byte; 32]),
        snapshot: SnapshotFingerprint::new([byte; 32]),
        carry: log.carry(),
    }
}

/// The signature of `record` by `key`, in the session of `terms()`.
fn signed_by(record: &Checkpoint, key: &Keypair) -> Signature {
    record.sign(&Secp256k1::new(), key, session_id(), &AUX)
}

/// The result of `log` that stops before its next tick, ended as `outcome`.
fn result(log: &SessionLog, outcome: Outcome) -> SessionResult {
    SessionResult {
        tick: log.next_tick(),
        outcome,
        state_hash: StateHash::new([9; 32]),
    }
}

/// Player 1 sends x and y stamped 1 and 2 before tick 0, which apply in ticks 1 and 2.
fn ahead() -> Vec<Sent<'static>> {
    resent(1, &[&[(1, b"x"), (2, b"y")]], 1)
}

/// A log of `header()` through ticks 0 and 1, player 1's y still to apply in tick 2.
fn two_ticks() -> SessionLog {
    let mut log = new_log();
    let mut applied = Vec::new();
    ahead()[0].submit(&mut log, &mut applied).unwrap();
    assert_eq!(seal(&mut log), []);
    assert_eq!(seal(&mut log), [(1, b"x".to_vec())]);
    log
}

#[test]
fn a_checkpoint_carries_the_logs_own_state_and_starts_the_next_segment() {
    let mut log = two_ticks();
    // Player 0 has sent nothing; player 1's chain holds x and y, stamped 1 and 2, one input
    // of stamp 2 so far, and y is due in tick 2, its last spill.
    let mut chain = InputChain::new(PlayerSlot::new(1), root(1));
    chain.extend(Tick::new(1), b"x");
    chain.extend(Tick::new(2), b"y");
    let player = |slot, chain| CarriedControl::Player {
        delegation: Box::new(delegation(slot)),
        chain,
    };
    let expected = LogCarry {
        slots: vec![
            CarriedSlot {
                control: player(0, InputChain::new(PlayerSlot::new(0), root(0))),
                leaver: None,
                stamps: StampCount::default(),
                spill: Spill::default(),
            },
            CarriedSlot {
                control: player(1, chain),
                leaver: None,
                stamps: StampCount {
                    last: Some(Tick::new(2)),
                    at_last: 1,
                    total: 2,
                },
                spill: Spill {
                    tick: Tick::new(2),
                    count: 1,
                },
            },
        ],
        pending: vec![CarriedInput {
            tick: Tick::new(2),
            slot: PlayerSlot::new(1),
            stamp: Tick::new(2),
            payload: b"y".to_vec(),
        }],
    };
    assert_eq!(log.carry(), expected);

    // The checkpoint at tick 2 starts segment 1; y still applies in tick 2, and the log runs on.
    let record = checkpoint(&log, 3);
    log.record_checkpoint(record.clone(), &signed_by(&record, &server_keypair()))
        .unwrap();
    assert_eq!(log.segment(), 1);
    assert_eq!(log.checkpoint_at(Tick::new(2)), Some(&record));
    assert_eq!(log.checkpoint_at(Tick::new(1)), None);
    assert_eq!(seal(&mut log), [(1, b"y".to_vec())]);
    let mut applied = Vec::new();
    resent(0, &[&[(3, b"z")]], 0)[0]
        .submit(&mut log, &mut applied)
        .unwrap();
    assert_eq!(seal(&mut log), [(0, b"z".to_vec())]);

    // The session ends before tick 4. The file holds both segments, the reveal of segment 1's
    // seed and the result, decodes to the same bytes, and replays the same ticks.
    let ended = result(&log, Outcome::Won { team: 1 });
    let signature = ended.sign(&Secp256k1::new(), &server_keypair(), session_id(), &AUX);
    log.record_result(ended, &signature).unwrap();
    log.reveal_seed(SEED_CHAIN.seed(1));
    assert_eq!(
        log.revealed_seeds().unwrap().seed(0),
        Some(SEED_CHAIN.seed(0))
    );
    let bytes = encoded(&log);
    let decoded = SessionLog::decode(&bytes).unwrap();
    assert_eq!(encoded(&decoded), bytes);
    assert_eq!(decoded.checkpoints().collect::<Vec<_>>(), [&record]);
    assert_eq!(decoded.result(), Some(&ended));
    let ticks: [&[(u32, &[u8])]; 4] = [&[], &[(1, b"x")], &[(1, b"y")], &[(0, b"z")]];
    assert_eq!(replayed(decoded).applied, per_tick(&ticks));
}

#[test]
fn a_checkpoint_or_a_result_the_log_does_not_hold_is_refused() {
    let mut log = two_ticks();
    let record = checkpoint(&log, 3);
    let stranger = Keypair::from_secret_key(&Secp256k1::new(), &secret(42));
    let other = |change: fn(&mut Checkpoint)| {
        let mut record = record.clone();
        change(&mut record);
        record
    };
    let flawed = [
        (record.clone(), stranger, CheckpointError::BadSignature),
        (
            other(|record| record.segment = 2),
            server_keypair(),
            CheckpointError::Segment,
        ),
        (
            other(|record| record.tick = Tick::new(3)),
            server_keypair(),
            CheckpointError::Tick,
        ),
        (
            other(|record| record.carry.pending.clear()),
            server_keypair(),
            CheckpointError::Carry,
        ),
    ];
    for (record, key, error) in flawed {
        let signature = signed_by(&record, &key);
        assert_eq!(log.record_checkpoint(record, &signature), Err(error));
        assert_eq!(log.segment(), 0);
    }
    // Once taken, a second checkpoint at the same tick would end a segment of no tick.
    log.record_checkpoint(record.clone(), &signed_by(&record, &server_keypair()))
        .unwrap();
    let again = checkpoint(&log, 4);
    let signature = signed_by(&again, &server_keypair());
    assert_eq!(
        log.record_checkpoint(again, &signature),
        Err(CheckpointError::Empty)
    );

    // A result signed by another key, or at another tick than the next.
    let ended = result(&log, Outcome::Draw);
    let secp = Secp256k1::new();
    let forged = ended.sign(&secp, &stranger, session_id(), &AUX);
    assert_eq!(
        log.record_result(ended, &forged),
        Err(ResultError::BadSignature)
    );
    let early = SessionResult {
        tick: Tick::new(1),
        ..ended
    };
    let signature = early.sign(&secp, &server_keypair(), session_id(), &AUX);
    assert_eq!(log.record_result(early, &signature), Err(ResultError::Tick));
    assert_eq!(log.result(), None);
}

#[test]
fn flawed_segments_in_a_log_file_are_refused() {
    // A log of two segments: its file decodes; with no segment, with the checkpoint signed by
    // another key, or revealing the first segment's seed, it does not.
    let mut log = two_ticks();
    let record = checkpoint(&log, 3);
    log.record_checkpoint(record.clone(), &signed_by(&record, &server_keypair()))
        .unwrap();
    log.reveal_seed(SEED_CHAIN.seed(1));
    let valid = encoded(&log);
    assert!(SessionLog::decode(&valid).is_ok());

    // The segment count follows the header, whose length a log of no tick gives: its file ends
    // with the segment count, the tick count 0, no entry, no reveal and no result.
    let count_at = encoded(&new_log()).len() - 5;
    assert_eq!(valid[count_at], 2);
    let mut no_segment = valid[..count_at].to_vec();
    no_segment.push(0);
    assert_eq!(
        SessionLog::decode(&no_segment).err(),
        Some(LogError::NoSegment)
    );

    let record_bytes = {
        let mut bytes = Vec::new();
        record.encode(&mut bytes);
        bytes
    };
    let record_at = valid
        .windows(record_bytes.len())
        .position(|window| window == record_bytes)
        .unwrap();
    let signature_at = record_at + record_bytes.len();
    let stranger = Keypair::from_secret_key(&Secp256k1::new(), &secret(42));
    let other = signed_by(&record, &stranger).to_bytes();
    let resigned = [&valid[..signature_at], &other, &valid[signature_at + 64..]].concat();
    assert_eq!(
        SessionLog::decode(&resigned).err(),
        Some(LogError::Checkpoint {
            segment: 1,
            error: CheckpointError::BadSignature,
        })
    );

    let seed_at = valid.len() - 1 - 32;
    let first_seed = [
        &valid[..seed_at],
        SEED_CHAIN.seed(0).as_bytes(),
        &valid[seed_at + 32..],
    ]
    .concat();
    assert_eq!(
        SessionLog::decode(&first_seed).err(),
        Some(LogError::WrongSeed)
    );
}

#[test]
#[should_panic(expected = "the server reveals the seed it committed to")]
fn revealing_an_earlier_segments_seed_is_a_bug() {
    let mut log = two_ticks();
    let record = checkpoint(&log, 3);
    log.record_checkpoint(record.clone(), &signed_by(&record, &server_keypair()))
        .unwrap();
    log.reveal_seed(SEED_CHAIN.seed(0));
}
