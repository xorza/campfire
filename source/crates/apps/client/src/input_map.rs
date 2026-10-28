use bevy::ecs::resource::Resource;
use bevy::ecs::system::Res;
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use campfire_client_files::{Chord, Key, KeyModifiers, KeysFile, Press};

/// The client's keys: the mode's `client/keys.toml`, or the engine's own for a mode of none,
/// each chord pressed when its key is and exactly its modifiers are held (02-engine-core.md, Keys
/// are the mode's data).
#[derive(Resource, Debug, Clone, Default, PartialEq)]
pub(crate) struct InputMap {
    file: KeysFile,
}

/// An action of the client that its chords do, any of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Act {
    Stop,
    DropBuild,
    Shop,
    Perks,
    Pause,
    Slower,
    Faster,
    Save,
    Load,
    Report,
    ScrollLeft,
    ScrollRight,
    ScrollUp,
    ScrollDown,
    ZoomIn,
    ZoomOut,
    TurnLeft,
    TurnRight,
    ResetCamera,
}

/// A row of chords, one for each slot in their order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChordRow {
    Cast,
    Learn,
    Items,
}

impl InputMap {
    pub(crate) const fn new(file: KeysFile) -> Self {
        Self { file }
    }

    /// What a press of a button is, the mode's rule of a drag and a click.
    pub(crate) const fn press(&self) -> Press {
        self.file.press
    }

    /// Whether a chord of `act` is held.
    pub(crate) fn is_held(&self, act: Act, keys: &ButtonInput<KeyCode>) -> bool {
        let modifiers = held_modifiers(keys);
        self.chords(act)
            .iter()
            .any(|chord| chord.modifiers == modifiers && keys.pressed(code(chord.key)))
    }

    /// Whether a chord of `act` went down this frame.
    pub(crate) fn pressed(&self, act: Act, keys: &ButtonInput<KeyCode>) -> bool {
        self.chords(act)
            .iter()
            .any(|&chord| Self::chord_pressed(chord, keys))
    }

    /// A run condition: whether a chord of `act` went down this frame.
    pub(crate) fn on(
        act: Act,
    ) -> impl FnMut(Res<'_, Self>, Res<'_, ButtonInput<KeyCode>>) -> bool + Clone {
        move |input: Res<'_, Self>, keys: Res<'_, ButtonInput<KeyCode>>| input.pressed(act, &keys)
    }

    /// The chords of `row`, by slot.
    pub(crate) fn row(&self, row: ChordRow) -> &[Chord] {
        match row {
            ChordRow::Cast => &self.file.cast,
            ChordRow::Learn => &self.file.learn,
            ChordRow::Items => &self.file.items,
        }
    }

    /// Whether `chord` went down this frame.
    pub(crate) fn chord_pressed(chord: Chord, keys: &ButtonInput<KeyCode>) -> bool {
        chord.modifiers == held_modifiers(keys) && keys.just_pressed(code(chord.key))
    }

    /// Whether `chord` went down this frame, which then no other reader of the frame sees go down.
    pub(crate) fn take_pressed(chord: Chord, keys: &mut ButtonInput<KeyCode>) -> bool {
        Self::chord_pressed(chord, keys) && keys.clear_just_pressed(code(chord.key))
    }

    /// Whether the modifiers that orbit the camera are exactly the ones held.
    pub(crate) fn orbits(&self, keys: &ButtonInput<KeyCode>) -> bool {
        held_modifiers(keys) == self.file.orbit
    }

    /// Whether the modifiers that add to the selection are held, others with them or not.
    pub(crate) fn adds(&self, keys: &ButtonInput<KeyCode>) -> bool {
        self.file.add.held_in(held_modifiers(keys))
    }

    /// Whether the modifiers that take a double click's type across the map are held, others
    /// with them or not.
    pub(crate) fn across_map(&self, keys: &ButtonInput<KeyCode>) -> bool {
        self.file.across_map.held_in(held_modifiers(keys))
    }

    fn chords(&self, act: Act) -> &[Chord] {
        let file = &self.file;
        match act {
            Act::Stop => &file.stop,
            Act::DropBuild => &file.drop_build,
            Act::Shop => &file.shop,
            Act::Perks => &file.perks,
            Act::Pause => &file.pause,
            Act::Slower => &file.slower,
            Act::Faster => &file.faster,
            Act::Save => &file.save,
            Act::Load => &file.load,
            Act::Report => &file.report,
            Act::ScrollLeft => &file.scroll_left,
            Act::ScrollRight => &file.scroll_right,
            Act::ScrollUp => &file.scroll_up,
            Act::ScrollDown => &file.scroll_down,
            Act::ZoomIn => &file.zoom_in,
            Act::ZoomOut => &file.zoom_out,
            Act::TurnLeft => &file.turn_left,
            Act::TurnRight => &file.turn_right,
            Act::ResetCamera => &file.reset_camera,
        }
    }
}

/// The modifiers held, each of either side.
fn held_modifiers(keys: &ButtonInput<KeyCode>) -> KeyModifiers {
    KeyModifiers {
        shift: keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]),
        control: keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]),
        alt: keys.any_pressed([KeyCode::AltLeft, KeyCode::AltRight]),
    }
}

/// Bevy's code of `key`, whose names are the W3C's too.
const fn code(key: Key) -> KeyCode {
    /// Maps each key named to Bevy's code of its name.
    macro_rules! codes {
        ($($name:ident),* $(,)?) => {
            match key {
                $(Key::$name => KeyCode::$name),*
            }
        };
    }
    codes! {
        Backquote, Backslash, BracketLeft, BracketRight, Comma,
        Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
        Equal,
        KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM,
        KeyN, KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
        Minus, Period, Quote, Semicolon, Slash,
        Backspace, Enter, Space, Tab,
        Delete, End, Home, Insert, PageDown, PageUp,
        ArrowDown, ArrowLeft, ArrowRight, ArrowUp,
        Numpad0, Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9,
        NumpadAdd, NumpadDecimal, NumpadDivide, NumpadEnter, NumpadMultiply, NumpadSubtract,
        Escape,
        F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_key_is_bevy_s_code_of_its_name() {
        for &key in Key::ALL {
            assert_eq!(format!("{:?}", code(key)), key.name());
        }
    }

    #[test]
    fn a_chord_is_pressed_with_exactly_its_modifiers() {
        let input = InputMap::default();
        let mut keys = ButtonInput::<KeyCode>::default();
        // Up alone scrolls; with Alt it zooms, and scrolls no more.
        keys.press(KeyCode::ArrowUp);
        assert!(input.is_held(Act::ScrollUp, &keys) && !input.is_held(Act::ZoomIn, &keys));
        keys.press(KeyCode::AltRight);
        assert!(!input.is_held(Act::ScrollUp, &keys) && input.is_held(Act::ZoomIn, &keys));
        // Q casts the first slot; with Control it learns it; with Shift too, it does neither.
        let mut keys = ButtonInput::<KeyCode>::default();
        let first = |row| input.row(row)[0];
        keys.press(KeyCode::KeyQ);
        assert!(InputMap::chord_pressed(first(ChordRow::Cast), &keys));
        assert!(!InputMap::chord_pressed(first(ChordRow::Learn), &keys));
        keys.clear();
        keys.press(KeyCode::ControlLeft);
        keys.release(KeyCode::KeyQ);
        keys.press(KeyCode::KeyQ);
        assert!(!InputMap::chord_pressed(first(ChordRow::Cast), &keys));
        assert!(InputMap::chord_pressed(first(ChordRow::Learn), &keys));
        assert!(input.orbits(&keys));
        keys.press(KeyCode::ShiftLeft);
        assert!(!InputMap::chord_pressed(first(ChordRow::Learn), &keys));
        assert!(!input.orbits(&keys));
        // Shift adds to the selection, Control with it or not; Alt takes the map.
        assert!(input.adds(&keys) && !input.across_map(&keys));
        keys.release(KeyCode::ShiftLeft);
        keys.press(KeyCode::AltLeft);
        assert!(!input.adds(&keys) && input.across_map(&keys));
        // S stops as it goes down, and held into the next frame stops no more.
        let mut keys = ButtonInput::<KeyCode>::default();
        keys.press(KeyCode::KeyS);
        assert!(input.pressed(Act::Stop, &keys));
        keys.clear();
        assert!(!input.pressed(Act::Stop, &keys) && keys.pressed(KeyCode::KeyS));
    }
}
