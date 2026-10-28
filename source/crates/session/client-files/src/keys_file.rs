use std::fmt;

use bevy_math::Vec2;
use campfire_values::PackagePath;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::client_file::{ClientFile, ClientFormat};
use crate::client_path;

/// A mode's `client/keys.toml`: the keys of each of the client's actions, each a list of
/// chords, any of which does it; a field the file does not give keeps the engine's own keys, as
/// a mode of no file does (02-engine-core.md, Keys are the mode's data).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct KeysFile {
    /// The chords that cast the avatar's abilities, one for each of its slots in their order.
    pub cast: Vec<Chord>,
    /// The chords that learn the next rank of the avatar's abilities, as `cast` gives them.
    pub learn: Vec<Chord>,
    /// The chords that cast the actions of the inventory's slots, one for each slot in their
    /// order: the inventory bar has a slot for each.
    pub items: Vec<Chord>,
    /// The chords that stop the selection.
    pub stop: Vec<Chord>,
    /// The chords that drop the build the player places.
    pub drop_build: Vec<Chord>,
    /// The chords that show and hide the shop.
    pub shop: Vec<Chord>,
    /// The chords that show and hide the panel of perks.
    pub perks: Vec<Chord>,
    /// The chords that pause a local match and play it on, step its speed down and up, save it,
    /// and load its latest save.
    pub pause: Vec<Chord>,
    pub slower: Vec<Chord>,
    pub faster: Vec<Chord>,
    pub save: Vec<Chord>,
    pub load: Vec<Chord>,
    /// The chords that write a report of what the client shows, for a bug the player sees
    /// (02-engine-core.md, A report is a folder of plain files).
    pub report: Vec<Chord>,
    /// The chords held that scroll the camera, zoom it and turn it, and the ones that set it
    /// back.
    pub scroll_left: Vec<Chord>,
    pub scroll_right: Vec<Chord>,
    pub scroll_up: Vec<Chord>,
    pub scroll_down: Vec<Chord>,
    pub zoom_in: Vec<Chord>,
    pub zoom_out: Vec<Chord>,
    pub turn_left: Vec<Chord>,
    pub turn_right: Vec<Chord>,
    pub reset_camera: Vec<Chord>,
    /// The modifiers held with a drag of the middle button that orbit the camera, where none
    /// pans it.
    pub orbit: KeyModifiers,
    /// The modifiers held with a click or a drag that add to the selection, or take away a click's
    /// units all selected, and with a double click that selects a type on the map, not the screen
    /// alone, as Zero Hour's prefer-selection key and its alt do (02-engine-core.md,
    /// Shift adds, a double click takes a kind).
    pub add: KeyModifiers,
    pub across_map: KeyModifiers,
    /// What a press of a button is, a selection's and the camera's.
    pub press: Press,
}

/// The one rule of every press of a button, a selection's and the camera's: it drags once the
/// cursor moves more than `drag` logical pixels along either axis from where the button went
/// down, as Zero Hour's `SelectionTranslator` drags past `Mouse.ini`'s `DragTolerance`, one that
/// does not drag is a click while it lasts at most `click` seconds, and a click on the unit the
/// last one clicked, at most `double` seconds after it, is a double click
/// (02-engine-core.md, Keys are the mode's data).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Press {
    pub drag: f32,
    pub click: f32,
    pub double: f32,
}

impl Press {
    /// Whether a press that went down at `from` drags with the cursor at `to`.
    pub fn drags(self, from: Vec2, to: Vec2) -> bool {
        (to - from).abs().max_element() > self.drag
    }

    /// Whether a press that did not drag, held for `seconds`, is a click.
    pub fn clicks(self, seconds: f64) -> bool {
        seconds <= f64::from(self.click)
    }

    /// Whether a click `seconds` after the last one on its unit is a double click.
    pub fn doubles(self, seconds: f64) -> bool {
        seconds <= f64::from(self.double)
    }
}

/// The engine's own press: Windows' default `SM_CXDRAG` and `SM_CYDRAG` of 4 pixels, so a hand
/// that shakes as it clicks still clicks, a click of up to 0.2 seconds, and Windows' default
/// double click time of 0.5 seconds (`GetDoubleClickTime`), by which Zero Hour's double click
/// comes.
impl Default for Press {
    fn default() -> Self {
        Self {
            drag: 4.0,
            click: 0.2,
            double: 0.5,
        }
    }
}

/// A package's file is untrusted, so a press of a length or a time not finite, or negative,
/// fails to read; a field it does not give keeps the engine's.
impl<'de> Deserialize<'de> for Press {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            drag: Option<f32>,
            click: Option<f32>,
            double: Option<f32>,
        }
        let Fields {
            drag,
            click,
            double,
        } = Fields::deserialize(deserializer)?;
        let engine = Self::default();
        let press = Self {
            drag: drag.unwrap_or(engine.drag),
            click: click.unwrap_or(engine.click),
            double: double.unwrap_or(engine.double),
        };
        let held = [press.drag, press.click, press.double]
            .iter()
            .all(|value| value.is_finite() && *value >= 0.0);
        if held {
            Ok(press)
        } else {
            Err(D::Error::custom(
                "a press's drag, click and double are finite and at least 0",
            ))
        }
    }
}

/// A key with the modifiers held with it, written as their names and its own joined by `+`, as
/// `Control+KeyQ`: it is pressed when its key is and its modifiers are exactly the ones held, as
/// Zero Hour's `MetaMap` matches a key's modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    pub key: Key,
    pub modifiers: KeyModifiers,
}

/// What a key's cap shows of a chord: its modifiers' names and its key's label, joined by `+`,
/// as `Control+Q`.
#[derive(Debug, Clone, Copy)]
pub struct ChordLabel(Chord);

/// The modifiers a chord holds, each of either side of the keyboard, written as their names
/// joined by `+`, as `Shift+Control`, and none as an empty text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyModifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
}

/// Declares the keys a chord may name, each by its name.
macro_rules! keys {
    ($($key:ident),* $(,)?) => {
        /// A key of the keyboard but a modifier, by the W3C's `KeyboardEvent.code` name, which
        /// names a key by its place, whatever a layout prints on it.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Key {
            $($key),*
        }

        impl Key {
            /// Every key, in the order the W3C's tables list them.
            pub const ALL: &[Self] = &[$(Self::$key),*];

            /// Its `KeyboardEvent.code` name.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$key => stringify!($key)),*
                }
            }
        }
    };
}

keys! {
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

impl KeysFile {
    /// Where a mode's package holds it.
    pub fn path() -> PackagePath {
        client_path::parse("client/keys.toml")
    }
}

/// The engine's own keys: Q, W, E and R cast and, with Control, learn; 1 to 6 cast the items; S
/// stops; Escape drops a build; P shows the shop; G shows the perks; Space pauses, minus and equals
/// or the keypad's minus and plus step the speed, F5 saves and F9 loads; the arrows scroll, and the
/// keypad's 8, 2, 4 and 6 zoom and turn, as the arrows do with Alt, for a keyboard with no keypad;
/// the keypad's 5 and Alt with Backspace set the camera back; Control with the middle button orbits
/// it; and Shift adds to the selection and Alt takes a double click's type across the map, as Zero
/// Hour's do (02-engine-core.md, Every key on a keyboard with no keypad).
impl Default for KeysFile {
    fn default() -> Self {
        let alt = KeyModifiers {
            alt: true,
            ..KeyModifiers::NONE
        };
        let control = KeyModifiers {
            control: true,
            ..KeyModifiers::NONE
        };
        let shift = KeyModifiers {
            shift: true,
            ..KeyModifiers::NONE
        };
        let plain = |keys: &[Key]| keys.iter().map(|&key| Chord::plain(key)).collect();
        let with = |key, modifiers| Chord { key, modifiers };
        let casts = [Key::KeyQ, Key::KeyW, Key::KeyE, Key::KeyR];
        Self {
            cast: plain(&casts),
            learn: casts.iter().map(|&key| with(key, control)).collect(),
            items: plain(&[
                Key::Digit1,
                Key::Digit2,
                Key::Digit3,
                Key::Digit4,
                Key::Digit5,
                Key::Digit6,
            ]),
            stop: plain(&[Key::KeyS]),
            drop_build: plain(&[Key::Escape]),
            shop: plain(&[Key::KeyP]),
            perks: plain(&[Key::KeyG]),
            pause: plain(&[Key::Space]),
            slower: plain(&[Key::Minus, Key::NumpadSubtract]),
            faster: plain(&[Key::Equal, Key::NumpadAdd]),
            save: plain(&[Key::F5]),
            load: plain(&[Key::F9]),
            report: plain(&[Key::F12]),
            scroll_left: plain(&[Key::ArrowLeft]),
            scroll_right: plain(&[Key::ArrowRight]),
            scroll_up: plain(&[Key::ArrowUp]),
            scroll_down: plain(&[Key::ArrowDown]),
            zoom_in: vec![Chord::plain(Key::Numpad8), with(Key::ArrowUp, alt)],
            zoom_out: vec![Chord::plain(Key::Numpad2), with(Key::ArrowDown, alt)],
            turn_left: vec![Chord::plain(Key::Numpad4), with(Key::ArrowLeft, alt)],
            turn_right: vec![Chord::plain(Key::Numpad6), with(Key::ArrowRight, alt)],
            reset_camera: vec![Chord::plain(Key::Numpad5), with(Key::Backspace, alt)],
            orbit: control,
            add: shift,
            across_map: alt,
            press: Press::default(),
        }
    }
}

impl Chord {
    /// `key` with no modifier.
    pub const fn plain(key: Key) -> Self {
        Self {
            key,
            modifiers: KeyModifiers::NONE,
        }
    }

    /// What a key's cap shows of it.
    pub const fn label(self) -> ChordLabel {
        ChordLabel(self)
    }

    /// The chord `text` writes; none for a key or a modifier it does not name, or a modifier
    /// named twice.
    fn parse(text: &str) -> Option<Self> {
        let (modifiers, key) = match text.rsplit_once('+') {
            Some((modifiers, key)) => (KeyModifiers::parse(modifiers)?, key),
            None => (KeyModifiers::NONE, text),
        };
        let key = Key::parse(key)?;
        Some(Self { key, modifiers })
    }
}

impl KeyModifiers {
    pub const NONE: Self = Self {
        shift: false,
        control: false,
        alt: false,
    };

    /// Whether each of its modifiers is held of `held`, and it names one; none is never held.
    pub const fn held_in(self, held: Self) -> bool {
        let any = self.shift || self.control || self.alt;
        any && (!self.shift || held.shift)
            && (!self.control || held.control)
            && (!self.alt || held.alt)
    }

    /// The modifiers `text` writes, none for an empty text; none for a name it does not know
    /// or one named twice.
    fn parse(text: &str) -> Option<Self> {
        let mut modifiers = Self::NONE;
        if text.is_empty() {
            return Some(modifiers);
        }
        for name in text.split('+') {
            let held = match name {
                "Shift" => &mut modifiers.shift,
                "Control" => &mut modifiers.control,
                "Alt" => &mut modifiers.alt,
                _ => return None,
            };
            if *held {
                return None;
            }
            *held = true;
        }
        Some(modifiers)
    }

    /// Each modifier held, with its name, in the order a chord writes them.
    fn named(self) -> impl Iterator<Item = &'static str> {
        [
            (self.shift, "Shift"),
            (self.control, "Control"),
            (self.alt, "Alt"),
        ]
        .into_iter()
        .filter_map(|(held, name)| held.then_some(name))
    }
}

impl Key {
    /// The key `name` names.
    fn parse(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|key| key.name() == name)
    }

    /// What a key's cap shows of it: a letter's or a digit's own, else its name.
    pub fn label(self) -> &'static str {
        let name = self.name();
        name.strip_prefix("Key")
            .or_else(|| name.strip_prefix("Digit"))
            .unwrap_or(name)
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for name in self.modifiers.named() {
            write!(f, "{name}+")?;
        }
        f.write_str(self.key.name())
    }
}

impl fmt::Display for ChordLabel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for name in self.0.modifiers.named() {
            write!(f, "{name}+")?;
        }
        f.write_str(self.0.key.label())
    }
}

impl fmt::Display for KeyModifiers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (at, name) in self.named().enumerate() {
            if at > 0 {
                f.write_str("+")?;
            }
            f.write_str(name)?;
        }
        Ok(())
    }
}

impl Serialize for Chord {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// A package's file is untrusted, so a chord of a key or a modifier no chord names fails to read.
impl<'de> Deserialize<'de> for Chord {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).ok_or_else(|| D::Error::custom("a chord is modifiers and a key by name"))
    }
}

impl Serialize for Key {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

/// A package's file is untrusted, so a key of no name fails to read.
impl<'de> Deserialize<'de> for Key {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).ok_or_else(|| D::Error::custom("a key by its `KeyboardEvent.code` name"))
    }
}

impl Serialize for KeyModifiers {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for KeyModifiers {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).ok_or_else(|| {
            D::Error::custom("modifiers are `Shift`, `Control` and `Alt`, each once")
        })
    }
}

impl ClientFile for KeysFile {
    const FORMAT: ClientFormat = ClientFormat::Toml;
}

#[cfg(test)]
mod tests {
    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn a_file_gives_its_own_chords_and_keeps_the_engine_s_for_the_rest() {
        let text = r#"
            cast = ["KeyA", "Shift+Control+KeyB"]
            stop = []
            orbit = "Alt"
        "#;
        let file: KeysFile = Toml::parse(text).unwrap();
        let both = KeyModifiers {
            shift: true,
            control: true,
            alt: false,
        };
        assert_eq!(
            file.cast,
            [
                Chord::plain(Key::KeyA),
                Chord {
                    key: Key::KeyB,
                    modifiers: both,
                }
            ]
        );
        assert!(file.stop.is_empty());
        assert_eq!(
            file.orbit,
            KeyModifiers {
                alt: true,
                ..KeyModifiers::NONE
            }
        );
        // A field it does not give is the engine's: the items are 1 to 6.
        let engine = KeysFile::default();
        assert_eq!(file.items, engine.items);
        assert_eq!(file.items.len(), 6);
        assert_ne!(file.cast, engine.cast);
        // Each writes as it reads, the engine's own too.
        assert_eq!(Toml::parse::<KeysFile>(&Toml::write(&file)).unwrap(), file);
        assert_eq!(
            Toml::parse::<KeysFile>(&Toml::write(&engine)).unwrap(),
            engine
        );
        assert!(Toml::write(&file).contains(r#""Shift+Control+KeyB""#));
        // A key no chord names, a modifier key, a modifier twice or unknown, a chord of no key,
        // and an unknown field fail to read.
        let refusal = |text: &str| {
            let refused = Toml::parse::<KeysFile>(text).unwrap_err();
            refused.message().to_owned()
        };
        for chord in [
            "KeyÄ",
            "ControlLeft",
            "Control+Control+KeyA",
            "Meta+KeyA",
            "Control+",
            "",
        ] {
            let text = format!("cast = [\"{chord}\"]");
            assert_eq!(
                refusal(&text),
                "a chord is modifiers and a key by name",
                "{chord}"
            );
        }
        assert_eq!(
            refusal("orbit = \"Shift+Shift\""),
            "modifiers are `Shift`, `Control` and `Alt`, each once"
        );
        let fire = refusal("fire = []");
        assert!(fire.starts_with("unknown field `fire`"), "{fire}");
    }

    #[test]
    fn a_key_reads_by_its_name_and_shows_its_cap() {
        for &key in Key::ALL {
            assert_eq!(Key::parse(key.name()), Some(key));
        }
        assert_eq!(Key::ALL.len(), 90);
        let labels = [Key::KeyQ, Key::Digit1, Key::F5, Key::Numpad8].map(Key::label);
        assert_eq!(labels, ["Q", "1", "F5", "Numpad8"]);
        let learn = KeysFile::default().learn[0];
        assert_eq!(
            (learn.to_string(), learn.label().to_string()),
            ("Control+KeyQ".to_owned(), "Control+Q".to_owned())
        );
    }
}
