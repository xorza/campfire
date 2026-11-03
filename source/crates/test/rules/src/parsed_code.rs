use std::cell::OnceCell;
use std::fmt;
use std::str::FromStr;

use proc_macro2::TokenStream;

use crate::named_path::NamedPath;

/// A view of a source file's code as the rules read it on one thread: its text, and its tokens,
/// its parse and the paths it names, each made once, on the first rule that asks, so every rule
/// of the file's pass shares them, as a `syn` tree crosses no thread (02-engine-core.md, The rules
/// are one test).
pub(crate) struct ParsedCode<'a> {
    path: &'a str,
    pub(crate) code: &'a str,
    tokens: OnceCell<TokenStream>,
    file: OnceCell<syn::File>,
    paths: OnceCell<Vec<NamedPath>>,
}

impl<'a> ParsedCode<'a> {
    /// The view `code` of the file at `path`, which a failure to lex or parse names.
    pub(crate) const fn new(path: &'a str, code: &'a str) -> Self {
        Self {
            path,
            code,
            tokens: OnceCell::new(),
            file: OnceCell::new(),
            paths: OnceCell::new(),
        }
    }

    /// Its tokens.
    pub(crate) fn tokens(&self) -> &TokenStream {
        self.tokens.get_or_init(|| {
            TokenStream::from_str(self.code)
                .unwrap_or_else(|error| panic!("{}: {error}", self.path))
        })
    }

    /// Its parse, from its tokens.
    pub(crate) fn file(&self) -> &syn::File {
        self.file.get_or_init(|| {
            syn::parse2(self.tokens().clone())
                .unwrap_or_else(|error| panic!("{}: {error}", self.path))
        })
    }

    /// The paths it names, in its order, from its tokens.
    pub(crate) fn paths(&self) -> &[NamedPath] {
        self.paths.get_or_init(|| NamedPath::of(self.tokens()))
    }
}

/// Its file's path, as `syn`'s tree has no `Debug` of its own in this build.
impl fmt::Debug for ParsedCode<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParsedCode")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}
