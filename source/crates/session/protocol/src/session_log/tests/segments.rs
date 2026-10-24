use super::*;

/// A change a test makes to a checkpoint record before the server key signs it.
type Forge = fn(&mut Checkpoint);

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
    TestKey::signer(*key).sign(&InSession {
        session_id: session_id(),
        record,
    })
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
    let player = |player_slot, chain| Control::Player {
        delegation: Box::new(delegation(player_slot)),
        chain,
    };
    let expected = LogCarry {
        player_slots: vec![
            CarriedPlayerSlot {
                control: player(0, InputChain::new(PlayerSlot::new(0), root(0))),
                leaver: None,
                stamps: StampCount::default(),
                spill: Spill::default(),
            },
            CarriedPlayerSlot {
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
            player_slot: PlayerSlot::new(1),
            stamp: Tick::new(2),
            payload: b"y".to_vec(),
        }],
    };
    assert_eq!(log.carry(), expected);

    // The checkpoint at tick 2 starts segment 1 as it begins, its record still to come; y still
    // applies in tick 2, and the log runs on. The record comes after tick 2.
    let record = checkpoint(&log, 3);
    let begun = log.begin_checkpoint(None).unwrap();
    assert_eq!(
        begun,
        CheckpointBegun {
            segment: 1,
            tick: Tick::new(2),
            carry: expected,
        }
    );
    assert_eq!(log.begun_checkpoint(), Some(&begun));
    assert_eq!(log.segment(), 1);
    assert_eq!(log.segment_starting(Tick::new(2)), Some(1));
    assert_eq!(log.segment_starting(Tick::new(0)), None);
    assert_eq!(log.segment_starting(Tick::new(1)), None);
    assert_eq!(log.checkpoint_at(Tick::new(2)), None);
    assert_eq!(seal(&mut log), [(1, b"y".to_vec())]);
    log.record_checkpoint(
        record.clone(),
        &signed_by(&record, &TestKey::server()),
        None,
    )
    .unwrap();
    assert_eq!(log.begun_checkpoint(), None);
    assert_eq!(log.checkpoint_at(Tick::new(2)), Some(&record));
    assert_eq!(log.checkpoint_at(Tick::new(1)), None);
    let mut applied = Vec::new();
    resent(0, &[&[(3, b"z")]], 0)[0]
        .submit(&mut log, &mut applied)
        .unwrap();
    assert_eq!(seal(&mut log), [(0, b"z".to_vec())]);

    // The session ends before tick 4. The file holds both segments, the reveal of segment 1's
    // seed and the result, decodes to the same bytes, and replays the same ticks.
    let ended = result(&log, Outcome::Won { team: 1 });
    let signature = TestKey::signer(TestKey::server()).sign(&InSession {
        session_id: session_id(),
        record: &ended,
    });
    log.record_result(ended, &signature, None).unwrap();
    log.reveal_seed(SEED_CHAIN.seed(1));
    assert_eq!(
        log.revealed_seeds().unwrap().seed(0).cloned(),
        Some(SEED_CHAIN.seed(0))
    );
    let bytes = encoded(&log);
    let decoded = SessionLog::decode(&bytes).unwrap();
    assert_eq!(encoded(&decoded), bytes);
    assert_eq!(decoded.checkpoints().collect::<Vec<_>>(), [&record]);
    assert_eq!(decoded.result(), Some(&ended));
    let ticks: [&[(u32, &[u8])]; 4] = [&[], &[(1, b"x")], &[(1, b"y")], &[(0, b"z")]];
    assert_eq!(replayed(decoded).applied, per_tick(&ticks));

    // The file from segment 1's checkpoint on starts at its record: it holds that record, the
    // result and the reveal, writes the same bytes again, and replays ticks 2 and 3 alone, y due
    // from the carry and z logged after it. It leaves out tick 0's and tick 1's entry.
    let mut cut = Vec::new();
    log.encode_from(1, &mut cut);
    let based = SessionLog::decode(&cut).unwrap();
    assert_eq!(based.starts_at(), Some(&record));
    assert_eq!(based.checkpoints().collect::<Vec<_>>(), [&record]);
    assert_eq!(
        (based.result(), based.next_tick(), based.segment()),
        (Some(&ended), Tick::new(4), 1)
    );
    assert_eq!(encoded(&based), cut);
    assert!(cut.len() < bytes.len());
    assert_eq!(replayed(based).applied, per_tick(&ticks[2..]));
    assert_eq!(SessionLog::decode(&bytes).unwrap().starts_at(), None);
}

#[test]
fn a_checkpoint_or_a_result_the_log_does_not_hold_is_refused() {
    let mut log = two_ticks();
    let record = checkpoint(&log, 3);
    let stranger = TestKey::of(42);
    let other = |change: fn(&mut Checkpoint)| {
        let mut record = record.clone();
        change(&mut record);
        record
    };
    let flawed = [
        (record.clone(), stranger, CheckpointError::BadSignature),
        (
            other(|record| record.segment = 2),
            TestKey::server(),
            CheckpointError::Segment,
        ),
        (
            other(|record| record.tick = Tick::new(3)),
            TestKey::server(),
            CheckpointError::Tick,
        ),
        (
            other(|record| record.carry.pending.clear()),
            TestKey::server(),
            CheckpointError::Carry,
        ),
    ];
    // A record with no checkpoint begun, and a second begin at the first's boundary, which would
    // end a segment of no tick.
    let sign = |record: &Checkpoint| signed_by(record, &TestKey::server());
    assert_eq!(
        log.record_checkpoint(record.clone(), &sign(&record), None),
        Err(CheckpointError::NotBegun)
    );
    log.begin_checkpoint(None).unwrap();
    assert_eq!(log.begin_checkpoint(None), Err(CheckpointError::Empty));
    for (record, key, error) in flawed {
        let signature = signed_by(&record, &key);
        assert_eq!(log.record_checkpoint(record, &signature, None), Err(error));
        assert_eq!(log.checkpoint_at(Tick::new(2)), None);
    }
    // A second checkpoint begins a tick later, before the first's record: the records come in
    // the order of their begins, and the latest begun is the one at the boundary.
    assert_eq!(seal(&mut log), [(1, b"y".to_vec())]);
    let second = checkpoint(&log, 4);
    let begun = log.begin_checkpoint(None).unwrap();
    assert_eq!((begun.segment, begun.tick), (2, Tick::new(3)));
    assert_eq!(log.begun_checkpoint(), Some(&begun));
    // A journal kept from here holds both begins, and the records as they come.
    let file = MemorySink::new();
    let mut journal = Journal::start(&log, file.boxed());
    assert_eq!(
        log.record_checkpoint(second.clone(), &sign(&second), Some(&mut journal)),
        Err(CheckpointError::Segment)
    );
    log.record_checkpoint(record.clone(), &sign(&record), Some(&mut journal))
        .unwrap();
    assert_eq!(log.begun_checkpoint(), Some(&begun));
    log.record_checkpoint(second.clone(), &sign(&second), Some(&mut journal))
        .unwrap();
    assert_eq!(log.begun_checkpoint(), None);
    assert_eq!(log.checkpoints().collect::<Vec<_>>(), [&record, &second]);
    drop(journal);
    let journal = file.bytes();
    let rebuilt = SessionLog::from_journal(JournalFrames::new(&journal).unwrap(), RELEASE).unwrap();
    assert_eq!(
        rebuilt.checkpoints().collect::<Vec<_>>(),
        [&record, &second]
    );
    // Once taken, a third checkpoint at the same tick would end a segment of no tick.
    assert_eq!(log.begin_checkpoint(None), Err(CheckpointError::Empty));

    // A result signed by another key, or at another tick than the next.
    let ended = result(&log, Outcome::Draw);
    let forged = TestKey::signer(stranger).sign(&InSession {
        session_id: session_id(),
        record: &ended,
    });
    assert_eq!(
        log.record_result(ended, &forged, None),
        Err(ResultError::BadSignature)
    );
    let early = SessionResult {
        tick: Tick::new(1),
        ..ended
    };
    let signature = TestKey::signer(TestKey::server()).sign(&InSession {
        session_id: session_id(),
        record: &early,
    });
    assert_eq!(
        log.record_result(early, &signature, None),
        Err(ResultError::Tick)
    );
    assert_eq!(log.result(), None);
}

#[test]
fn flawed_segments_in_a_log_file_are_refused() {
    // A log of two segments: its file decodes; with no segment, with the checkpoint signed by
    // another key, or revealing the first segment's seed, it does not.
    let mut log = two_ticks();
    let record = checkpoint(&log, 3);
    log.begin_checkpoint(None).unwrap();
    log.record_checkpoint(
        record.clone(),
        &signed_by(&record, &TestKey::server()),
        None,
    )
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
    let stranger = TestKey::of(42);
    let other = signed_by(&record, &stranger).to_bytes();
    let resigned = [&valid[..signature_at], &other, &valid[signature_at + 64..]].concat();
    assert_eq!(
        SessionLog::decode(&resigned).err(),
        Some(LogError::Checkpoint {
            segment: 1,
            error: CheckpointError::BadSignature,
        })
    );

    // The file from segment 1's checkpoint on: a byte of its carry changed, or its record
    // signed by another key, it fails the record's signature; its first segment named as 2, its
    // record is of another; a carry the server key signs with an input due before the boundary,
    // or with one player slot of the header's two, is no state a log holds.
    let mut based = Vec::new();
    log.encode_from(1, &mut based);
    assert!(SessionLog::decode(&based).is_ok());
    let based_at = based
        .windows(record_bytes.len())
        .position(|window| window == record_bytes)
        .unwrap();
    let refused = |error| Some(LogError::Checkpoint { segment: 1, error });
    let mut changed = based.clone();
    let payload_at = based_at + record_bytes.len() - 1;
    assert_eq!(changed[payload_at], b'y');
    changed[payload_at] = b'z';
    assert_eq!(
        SessionLog::decode(&changed).err(),
        refused(CheckpointError::BadSignature)
    );
    let signature_at = based_at + record_bytes.len();
    let resigned = [&based[..signature_at], &other, &based[signature_at + 64..]].concat();
    assert_eq!(
        SessionLog::decode(&resigned).err(),
        refused(CheckpointError::BadSignature)
    );
    let mut renumbered = based.clone();
    assert_eq!(renumbered[based_at - 2..based_at], [1, 1]);
    renumbered[based_at - 2] = 2;
    assert_eq!(
        SessionLog::decode(&renumbered).err(),
        Some(LogError::Checkpoint {
            segment: 2,
            error: CheckpointError::Segment,
        })
    );
    let forged = |change: fn(&mut Checkpoint)| {
        let mut forged = record.clone();
        change(&mut forged);
        let mut bytes = Vec::new();
        forged.encode(&mut bytes);
        let signature = signed_by(&forged, &TestKey::server()).to_bytes();
        let file = [
            &based[..based_at],
            &bytes,
            &signature,
            &based[signature_at + 64..],
        ]
        .concat();
        SessionLog::decode(&file).err()
    };
    // Each flaw of a carry the server key signs is refused with its own: an input due before the
    // boundary; one player slot of the header's two; player 1's stamps counting u64::MAX inputs,
    // past the 2 of its chain, or 3 of one stamp, past the max of 2 a tick; player 0's main key on
    // player slot 1 too; a spill of 3; an input of player slot 0 after one of player slot 1 in the
    // same tick; a payload past the max; three inputs of one player slot in one tick; and an input
    // of an open player slot.
    let player_slot = PlayerSlot::new;
    let flaws: [(Forge, CarryFlaw); 12] = [
        (
            |record| record.carry.pending[0].tick = Tick::new(1),
            CarryFlaw::Pending(0),
        ),
        (
            |record| {
                record.carry.pending[0].tick = record.tick.after(Ticks::new(MAX_LEAD + 1));
            },
            CarryFlaw::Pending(0),
        ),
        (
            |record| {
                record.carry.player_slots[1].spill = Spill {
                    tick: record.tick.after(Ticks::new(MAX_LEAD + 1)),
                    count: 1,
                };
            },
            CarryFlaw::Spill(player_slot(1)),
        ),
        (
            |record| drop(record.carry.player_slots.pop()),
            CarryFlaw::PlayerSlots,
        ),
        (
            |record| record.carry.player_slots[1].stamps.total = u64::MAX,
            CarryFlaw::Chain(player_slot(1)),
        ),
        (
            |record| record.carry.player_slots[1].stamps.at_last = MAX_INPUTS_PER_TICK + 1,
            CarryFlaw::Stamps(player_slot(1)),
        ),
        (
            |record| {
                let first = record.carry.player_slots[0].control.clone();
                if let (
                    Control::Player { delegation, .. },
                    Control::Player {
                        delegation: second, ..
                    },
                ) = (first, &mut record.carry.player_slots[1].control)
                {
                    *second = delegation;
                }
            },
            CarryFlaw::MainKeyHeld(player_slot(1)),
        ),
        (
            |record| record.carry.player_slots[1].spill.count = MAX_INPUTS_PER_TICK + 1,
            CarryFlaw::Spill(player_slot(1)),
        ),
        (
            |record| {
                let mut other = record.carry.pending[0].clone();
                other.player_slot = PlayerSlot::new(0);
                record.carry.pending.push(other);
            },
            CarryFlaw::Pending(1),
        ),
        (
            |record| {
                record.carry.pending[0].payload = vec![0; MAX_PAYLOAD_LEN as usize + 1];
            },
            CarryFlaw::Pending(0),
        ),
        (
            |record| {
                let input = record.carry.pending[0].clone();
                record
                    .carry
                    .pending
                    .extend(iter::repeat_n(input, MAX_INPUTS_PER_TICK as usize));
            },
            CarryFlaw::Pending(MAX_INPUTS_PER_TICK as usize),
        ),
        (
            |record| {
                record.carry.player_slots[0].control = Control::Open;
                let mut open = record.carry.pending[0].clone();
                open.player_slot = PlayerSlot::new(0);
                record.carry.pending.insert(0, open);
            },
            CarryFlaw::Pending(0),
        ),
    ];
    for (change, flaw) in flaws {
        assert_eq!(forged(change), refused(CheckpointError::BadCarry(flaw)));
    }

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
    log.begin_checkpoint(None).unwrap();
    log.record_checkpoint(
        record.clone(),
        &signed_by(&record, &TestKey::server()),
        None,
    )
    .unwrap();
    log.reveal_seed(SEED_CHAIN.seed(0));
}

#[test]
fn a_load_goes_back_to_its_checkpoint_and_the_journal_follows_it() {
    // A save at tick 2, segment 1, then tick 2 runs y, and player 0 sends z for tick 3; the log
    // keeps a journal, so it forgets what came before the save, and its twin keeps none.
    let mut log = two_ticks();
    let mut twin = two_ticks();
    let file = MemorySink::new();
    let mut journal = Journal::start(&log, file.boxed());
    let record = checkpoint(&log, 3);
    let signature = signed_by(&record, &TestKey::server());
    log.begin_checkpoint(Some(&mut journal)).unwrap();
    log.record_checkpoint(record.clone(), &signature, Some(&mut journal))
        .unwrap();
    twin.begin_checkpoint(None).unwrap();
    twin.record_checkpoint(record.clone(), &signature, None)
        .unwrap();
    assert_eq!((log.base(), twin.base()), (Some(1), None));
    let saved = encoded(&twin);
    let sealed: Vec<_> = log
        .seal_tick(Some(&mut journal))
        .map(|input| (input.player_slot.get(), input.payload.to_vec()))
        .collect();
    assert_eq!(sealed, [(1, b"y".to_vec())]);
    assert_eq!(seal(&mut twin), [(1, b"y".to_vec())]);
    let mut applied = Vec::new();
    let z = resent(0, &[&[(3, b"z")]], 0);
    z[0].submit_to(&mut log, &mut applied, Some(&mut journal))
        .unwrap();
    z[0].submit(&mut twin, &mut applied).unwrap();

    // Segment 0 starts from no checkpoint, and segment 2 is none of the log's.
    for segment in [0, 2] {
        assert_eq!(
            log.load(segment, Some(&mut journal)),
            Err(LogLoadError::NoCheckpoint)
        );
    }
    // The load's record, 4 + 1 + 1 + 32 = 38 bytes framed, needs its room and the end's in the
    // journal.
    let start = journal.len();
    log.bounds.journal = start + 38 + 4 * log.unit - 1;
    assert_eq!(log.load(1, Some(&mut journal)), Err(LogLoadError::LogFull));
    log.bounds.journal += 1;
    // The load of segment 1, from its checkpoint's carry: the log is the one saved, at tick 2
    // again, its y due again there and z gone, as its twin's file shows; and the journal
    // rebuilds the log loaded.
    log.load(1, Some(&mut journal)).unwrap();
    twin.load(1, None).unwrap();
    assert_eq!(journal.len(), start + 38);
    log.bounds = Bounds::MAX;
    assert_eq!(encoded(&twin), saved);
    for loaded in [&log, &twin] {
        assert_eq!((loaded.next_tick(), loaded.segment()), (Tick::new(2), 1));
        assert_eq!(loaded.checkpoint_at(Tick::new(2)), Some(&record));
        assert_eq!(loaded.carry(), record.carry);
        assert_eq!(loaded.file_len, len(saved.len()));
    }
    journal.flush();
    let loaded = file.bytes();

    // The journal counts its records on past the load: z again, in the record after the load's,
    // is durable once the records through it are.
    let written = journal.written();
    z[0].submit_to(&mut log, &mut applied, Some(&mut journal))
        .unwrap();
    z[0].submit(&mut twin, &mut applied).unwrap();
    let player_slot = PlayerSlot::new(0);
    journal.advance_durable(written);
    assert_eq!(journal.durable_head(player_slot), None);
    journal.advance_durable(written + 1);
    let Some(Controller::Player { chain, .. }) = log.controller(player_slot) else {
        panic!("player 0 controls player slot 0");
    };
    assert_eq!(
        journal
            .durable_head(player_slot)
            .map(|head| (head.seq, head.head)),
        Some((chain.next_seq() - 1, chain.head()))
    );
    // The journal publishes the log loaded and gone on, z once after the load: its twin's file.
    twin.reveal_seed(SEED_CHAIN.seed(1));
    drop(log);
    drop(journal);
    assert_eq!(published_from(&file.bytes()), Ok(encoded(&twin)));

    // The journal's records to the load rebuild the log loaded, which forgets what came before
    // its save.
    let records: Vec<&[u8]> = JournalFrames::new(&loaded).unwrap().collect();
    assert_eq!(records.last().map(|record| record[0]), Some(7));
    let rebuilt = SessionLog::from_journal(records, RELEASE).unwrap();
    assert_eq!(
        (rebuilt.next_tick(), rebuilt.base()),
        (Tick::new(2), Some(1))
    );
    assert_eq!(rebuilt.carry(), record.carry);
}
