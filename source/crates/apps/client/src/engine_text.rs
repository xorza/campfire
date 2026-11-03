use std::fmt;

use bevy::ecs::resource::Resource;
use campfire_client_files::PanelCommand;
use campfire_common::EnumAll;
use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use unic_langid::LanguageIdentifier;

/// The engine's own messages, which no package gives, in Mozilla's Fluent as a package's text is
/// (03-game-scripting.md, No human text in data or scripts): the client's `locale/en.ftl`, built
/// into it, in English, the one language the engine gives yet.
#[derive(Resource)]
pub(crate) struct EngineText {
    bundle: FluentBundle<FluentResource>,
}

/// A message of the engine's own, each a message of its file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub(crate) enum EngineMessage {
    Victory,
    Defeat,
    MatchEnded,
    Sell,
    Stop,
    AttackMove,
    Rally,
    Exit,
    Evacuate,
    /// A time in whole seconds, `seconds`.
    Seconds,
    /// A cast on its cooldown, ready in `seconds`.
    ReadyIn,
    /// A research held.
    Done,
    /// A player's rank, `level`, and its experience, `xp`.
    Rank,
    /// A player's points to spend, `points`.
    Points,
    /// A perk held.
    Held,
}

/// The engine's English file.
const ENGLISH: &str = include_str!("../locale/en.ftl");

impl EngineMessage {
    /// The name of the panel's `command`.
    pub(crate) const fn of(command: PanelCommand) -> Self {
        match command {
            PanelCommand::Sell => Self::Sell,
            PanelCommand::Stop => Self::Stop,
            PanelCommand::AttackMove => Self::AttackMove,
            PanelCommand::Rally => Self::Rally,
            PanelCommand::Exit => Self::Exit,
            PanelCommand::Evacuate => Self::Evacuate,
        }
    }

    /// Its id in the engine's file.
    const fn id(self) -> &'static str {
        match self {
            Self::Victory => "match-victory",
            Self::Defeat => "match-defeat",
            Self::MatchEnded => "match-ended",
            Self::Sell => "command-sell",
            Self::Stop => "command-stop",
            Self::AttackMove => "command-attack-move",
            Self::Rally => "command-rally",
            Self::Exit => "command-exit",
            Self::Evacuate => "command-evacuate",
            Self::Seconds => "time-seconds",
            Self::ReadyIn => "button-ready",
            Self::Done => "button-done",
            Self::Rank => "perk-rank",
            Self::Points => "perk-points",
            Self::Held => "perk-held",
        }
    }
}

impl EngineText {
    /// The engine's messages in English.
    pub(crate) fn english() -> Self {
        let resource = FluentResource::try_new(ENGLISH.to_owned())
            .expect("the engine's file reads, as a test proves");
        let language: LanguageIdentifier = "en".parse().expect("`en` is a language");
        let mut bundle = FluentBundle::new_concurrent(vec![language]);
        // A window's title shows no isolation marks around a placeable.
        bundle.set_use_isolating(false);
        bundle
            .add_resource(resource)
            .expect("the engine's file defines each message once, as a test proves");
        Self { bundle }
    }

    /// The text of `message`, which takes no argument.
    pub(crate) fn text(&self, message: EngineMessage) -> String {
        let mut text = String::new();
        self.write(message, &[], &mut text);
        text
    }

    /// Appends to `out` the text of `message`, each of its arguments by its name in `args`, as
    /// its value writes itself.
    pub(crate) fn write(
        &self,
        message: EngineMessage,
        args: &[(&'static str, &dyn fmt::Display)],
        out: &mut String,
    ) {
        let pattern = self
            .bundle
            .get_message(message.id())
            .and_then(|message| message.value())
            .expect("the engine's file gives each message a value, as a test proves");
        let mut fluent = FluentArgs::new();
        for &(name, value) in args {
            fluent.set(name, value.to_string());
        }
        let mut errors = Vec::new();
        let given = (!args.is_empty()).then_some(&fluent);
        out.push_str(&self.bundle.format_pattern(pattern, given, &mut errors));
        debug_assert!(errors.is_empty(), "{errors:?}");
    }
}

impl fmt::Debug for EngineText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EngineText").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use fluent_syntax::ast::Entry;

    use super::*;

    #[test]
    fn the_engine_s_file_gives_each_of_its_messages_and_no_other() {
        // Each message of an argument takes 1, and the rank 3 and 450.
        let text = EngineText::english();
        let (one, three, many) = (1, 3, 450);
        let args = |message| -> Vec<(&'static str, &dyn fmt::Display)> {
            match message {
                EngineMessage::Seconds | EngineMessage::ReadyIn => vec![("seconds", &one)],
                EngineMessage::Rank => vec![("level", &three), ("xp", &many)],
                EngineMessage::Points => vec![("points", &one)],
                _ => Vec::new(),
            }
        };
        let shown = EngineMessage::ALL.map(|message| {
            let mut shown = String::new();
            text.write(message, &args(message), &mut shown);
            shown
        });
        assert_eq!(
            shown,
            [
                "Campfire: victory",
                "Campfire: defeat",
                "Campfire: the match ended",
                "Sell",
                "Stop",
                "Attack move",
                "Set rally point",
                "Exit",
                "Evacuate",
                "1 s",
                "ready in 1 s",
                "done",
                "rank 3 · 450 xp",
                "1 points",
                "held",
            ]
        );
        let resource = FluentResource::try_new(ENGLISH.to_owned()).unwrap();
        let ids: Vec<&str> = resource
            .entries()
            .filter_map(|entry| match entry {
                Entry::Message(message) => Some(message.id.name),
                _ => None,
            })
            .collect();
        assert_eq!(ids, EngineMessage::ALL.map(EngineMessage::id));
    }
}
