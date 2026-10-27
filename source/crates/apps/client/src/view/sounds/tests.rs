use std::num::{NonZeroU8, NonZeroU16};

use campfire_mode::Offers;

use super::*;

#[test]
fn a_sound_fades_to_its_ranges_end_and_the_zoom_raises_a_near_camera() {
    // Full to 175 m and silent at 800: halfway, 487.5 m, 175 / 487.5 of the release's fall-off,
    // times the fade's 1 − (312.5 / 625)⁴ = 15/16; at 350 m, half, times 1 − (175 / 625)⁴.
    assert_eq!(fade(0.0, 175.0, 800.0), 1.0);
    assert_eq!(fade(175.0, 175.0, 800.0), 1.0);
    assert_eq!(fade(487.5, 175.0, 800.0), 175.0 / 487.5 * (15.0 / 16.0));
    assert_eq!(
        fade(350.0, 175.0, 800.0),
        0.5 * (1.0 - (175.0_f32 / 625.0).powi(4))
    );
    assert_eq!(fade(800.0, 175.0, 800.0), 0.0);
    assert_eq!(fade(5000.0, 175.0, 800.0), 0.0);
    // The zoom: all within 130 m, 1 − 0.2 from 425 m, and halfway, 277.5 m, 1 − 0.1.
    let zoom = ZoomBoost {
        near: 130.0,
        far: 425.0,
        boost: 0.25,
    };
    assert_eq!(
        [100.0, 277.5, 425.0, 900.0].map(|distance| zoom_volume(distance, zoom)),
        [1.0, 1.0 - 0.125, 0.75, 0.75]
    );
}

#[test]
fn the_microphone_stands_its_height_above_the_ground_or_its_share_of_the_way() {
    let listener = Listener {
        height: 50.0,
        share: 0.25,
    };
    let ground = Vec3::new(10.0, 4.0, -20.0);
    // A camera 400 m up and 300 m back: 50 m up is an eighth of the way, nearer than a quarter.
    let high = ground + Vec3::new(0.0, 400.0, 300.0);
    assert_eq!(
        microphone(ground, high, Some(listener)),
        ground + Vec3::new(0.0, 50.0, 37.5)
    );
    // A camera 100 m up: a quarter of the way, 25 m up, is nearer than 50 m.
    let low = ground + Vec3::new(0.0, 100.0, 80.0);
    assert_eq!(
        microphone(ground, low, Some(listener)),
        ground + Vec3::new(0.0, 25.0, 20.0)
    );
    // A camera below its height above the ground takes the share; a mode of no listener hears
    // at the camera.
    let near = ground + Vec3::new(0.0, 40.0, 40.0);
    assert_eq!(
        microphone(ground, near, Some(listener)),
        ground + Vec3::new(0.0, 10.0, 10.0)
    );
    assert_eq!(microphone(ground, high, None), high);
    // The plane at height 0 under a camera 10 m up looking down at 45°.
    let at = ground_plane(Vec3::new(0.0, 10.0, 0.0), Vec3::new(0.0, -1.0, -1.0));
    assert_eq!(at, Some(Vec3::new(0.0, 0.0, -10.0)));
    assert_eq!(ground_plane(Vec3::Y, Vec3::Y), None);
}

#[test]
fn a_pick_draws_never_the_last_file_again_of_more_than_two_or_takes_each_in_turn() {
    // Of three files, a draw of the last one drawn takes the next; of two, it stands.
    let mut pick = FilePick::default();
    assert_eq!(
        [1, 1, 2, 0].map(|drawn| pick.next(SoundPick::Random, 3, drawn)),
        [1, 2, 0, 1]
    );
    let mut two = FilePick::default();
    assert_eq!(
        [1, 1].map(|drawn| two.next(SoundPick::Random, 2, drawn)),
        [1, 1]
    );
    // In turn, from the first, whatever the draw.
    let mut turn = FilePick::default();
    assert_eq!(
        [2, 2, 2, 2].map(|drawn| turn.next(SoundPick::InTurn, 3, drawn)),
        [0, 1, 2, 0]
    );
}

#[test]
fn a_limit_drops_a_play_past_its_count_or_stops_the_oldest() {
    let limit = |count, interrupt| {
        Some(SoundLimit {
            count: NonZeroU16::new(count).unwrap(),
            interrupt,
        })
    };
    // Two plays alive, at places 1 and 4, started 7th and 3rd.
    let same = [(1, 7), (4, 3)];
    assert_eq!(room(None, same.into_iter()), Room::Free);
    assert_eq!(room(limit(3, false), same.into_iter()), Room::Free);
    assert_eq!(room(limit(2, false), same.into_iter()), Room::Full);
    // Interrupting, the 3rd started stops; of none alive, a limit of one has room.
    assert_eq!(room(limit(2, true), same.into_iter()), Room::Oldest(4));
    assert_eq!(room(limit(1, true), [].into_iter()), Room::Free);
}

#[test]
fn a_sound_is_heard_by_its_owner_its_allies_its_enemies_or_everyone() {
    let (me, other) = (PlayerSlot::new(0), PlayerSlot::new(1));
    let (mine, friend, foe) = (Team::new(0), Team::new(1), Team::new(2));
    let relation = |team: Team| {
        Some(match team.index() {
            0 | 1 => Relation::Friendly,
            _ => Relation::Hostile,
        })
    };
    let source = |owner, team| Some(SoundSource { owner, team });
    let heard = |heard_by, source| hears(heard_by, source, Some(me), relation);
    // The owner's alone: the player's own unit's, not an ally's.
    assert!(heard(Hearers::Owner, source(Some(me), mine)));
    assert!(!heard(Hearers::Owner, source(Some(other), friend)));
    // Allies' but the owner's: an ally's, not the player's own nor an enemy's.
    assert!(heard(Hearers::Allies, source(Some(other), friend)));
    assert!(!heard(Hearers::Allies, source(Some(me), mine)));
    assert!(!heard(Hearers::Allies, source(Some(other), foe)));
    // Enemies': an enemy's alone; everyone's, any; a sound of no source, by anyone.
    assert!(heard(Hearers::Enemies, source(Some(other), foe)));
    assert!(!heard(Hearers::Enemies, source(Some(other), friend)));
    assert!(heard(Hearers::Everyone, source(None, foe)));
    assert!(heard(Hearers::Owner, None));
    // A spectator owns nothing.
    assert!(!hears(
        Hearers::Owner,
        source(Some(me), mine),
        None,
        relation
    ));
}

#[test]
fn the_music_plays_the_playlist_of_the_players_pick() {
    let name = |text: &str| DeclaredName::new(text).unwrap();
    let path = |text: &str| PackagePath::parse(text).unwrap();
    let choice = ChoiceData {
        name: campfire_values::TextId::new("choice-faction").unwrap(),
        offers: Offers::Values,
        unique: false,
        count: NonZeroU8::MIN,
        slot: None,
        values: ["america", "china"].map(name).into(),
    };
    let choices = BTreeMap::from([(name("faction"), choice)]);
    let usa = vec![
        path("client/music/usa_01.mp3"),
        path("client/music/silence60.mp3"),
    ];
    let music = Music {
        choice: name("faction"),
        playlists: BTreeMap::from([(name("america"), usa.clone())]),
    };
    // Player slot 0 picked America, player slot 1 China, of no playlist, player slot 2 none.
    let picks = [0, 1, PlayerSlotPicks::NONE];
    let played = |player_slot| playlist(&music, &choices, &picks, PlayerSlot::new(player_slot));
    assert_eq!(played(0), Some(&*usa));
    assert_eq!(played(1), None);
    assert_eq!(played(2), None);
    // Terms of no whole row give none.
    assert_eq!(playlist(&music, &choices, &[], PlayerSlot::new(0)), None);
}
