use std::collections::BTreeSet;
use std::sync::Arc;

use campfire_content::{MessageId, PackagePath};
use fluent_bundle::FluentResource;
use fluent_syntax::ast::Entry;

use crate::error::{LoadProblem, LocaleProblem};
use crate::package_files::PackageFiles;

/// One Fluent file of human text, parsed, with the messages it defines.
#[derive(Debug)]
pub(crate) struct LocaleFile {
    pub(crate) path: PackagePath,
    pub(crate) resource: Arc<FluentResource>,
    /// Every message it defines.
    messages: BTreeSet<MessageId>,
    /// The messages it gives a value, which data may name; a message of attributes alone has
    /// none.
    values: BTreeSet<MessageId>,
}

impl LocaleFile {
    /// The file at `path` of `files`: it parses with no error, and defines no message or term
    /// twice.
    pub(crate) fn read(
        files: &PackageFiles,
        path: &PackagePath,
    ) -> Result<LocaleFile, LoadProblem> {
        let fail = |problem| LoadProblem::Locale {
            path: path.clone(),
            problem,
        };
        let text = files.read_text(path).map_err(LoadProblem::Content)?;
        let resource = FluentResource::try_new(text.to_owned()).map_err(|(_, errors)| {
            fail(LocaleProblem::Parse(
                errors
                    .into_iter()
                    .next()
                    .expect("a failed parse has an error"),
            ))
        })?;
        let mut messages = BTreeSet::new();
        let mut values = BTreeSet::new();
        let mut terms = BTreeSet::new();
        let id = |name| MessageId::new(name).expect("Fluent's identifiers are message ids");
        for entry in resource.entries() {
            match entry {
                Entry::Message(message) => {
                    let id = id(message.id.name);
                    if message.value.is_some() {
                        values.insert(id.clone());
                    }
                    if !messages.insert(id.clone()) {
                        return Err(fail(LocaleProblem::Repeated(id)));
                    }
                }
                Entry::Term(term) => {
                    let id = id(term.id.name);
                    if !terms.insert(id.clone()) {
                        return Err(fail(LocaleProblem::Repeated(id)));
                    }
                }
                Entry::Comment(_)
                | Entry::GroupComment(_)
                | Entry::ResourceComment(_)
                | Entry::Junk { .. } => {}
            }
        }
        Ok(LocaleFile {
            path: path.clone(),
            resource: Arc::new(resource),
            messages,
            values,
        })
    }

    /// Whether it gives message `id` a value.
    pub(crate) fn gives(&self, id: &str) -> bool {
        self.values.contains(id)
    }

    /// A message it defines that `own` does not, if it has one: a translation of `own` names
    /// none.
    pub(crate) fn stray(&self, own: Option<&LocaleFile>) -> Option<&MessageId> {
        self.messages
            .iter()
            .find(|id| own.is_none_or(|own| !own.messages.contains(*id)))
    }
}
