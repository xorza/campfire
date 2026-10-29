use std::fmt;

use bevy::ecs::resource::Resource;
use campfire_client_files::PanelCommand;
use fluent_bundle::FluentResource;
use fluent_bundle::concurrent::FluentBundle;
use unic_langid::LanguageIdentifier;

/// The engine's own messages, which no package gives, in Mozilla's Fluent as a package's text is
/// (03-game-scripting.md, No human text in data or scripts): the client's `locale/en.ftl`, built
/// into it, in English, the one language the engine gives yet.
#[derive(Resource)]
pub(crate) struct EngineText {
    bundle: FluentBundle<FluentResource>,
}

/// A message of the engine's own, each a message of its file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    /// The text of `message`.
    pub(crate) fn text(&self, message: EngineMessage) -> String {
        let pattern = self
            .bundle
            .get_message(message.id())
            .and_then(|message| message.value())
            .expect("the engine's file gives each message a value, as a test proves");
        let mut errors = Vec::new();
        let text = self.bundle.format_pattern(pattern, None, &mut errors);
        debug_assert!(errors.is_empty(), "{errors:?}");
        text.into_owned()
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

    /// Every message, so one no test lists fails the text's check.
    const ALL: [EngineMessage; 9] = [
        EngineMessage::Victory,
        EngineMessage::Defeat,
        EngineMessage::MatchEnded,
        EngineMessage::Sell,
        EngineMessage::Stop,
        EngineMessage::AttackMove,
        EngineMessage::Rally,
        EngineMessage::Exit,
        EngineMessage::Evacuate,
    ];

    #[test]
    fn the_engine_s_file_gives_each_of_its_messages_and_no_other() {
        let text = EngineText::english();
        let shown = ALL.map(|message| text.text(message));
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
        assert_eq!(ids, ALL.map(EngineMessage::id));
    }
}
