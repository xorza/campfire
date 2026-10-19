use std::path::PathBuf;

use clap::Parser;

/// Imports the Zero Hour install `GENERALSZH_DATA` names and checks what it gives.
#[derive(Debug, Parser)]
#[command(version)]
pub(crate) struct CommandLine {
    /// The directory the import's packages go to, in place of those an earlier run wrote there;
    /// `zh-package` in the home directory when none is given
    pub(crate) out: Option<PathBuf>,
}

impl CommandLine {
    /// The directory the packages go to: the one given, or `zh-package` in `home`; none when
    /// neither names one.
    pub(crate) fn out_dir(self, home: Option<PathBuf>) -> Option<PathBuf> {
        self.out
            .or_else(|| home.map(|home| home.join("zh-package")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_packages_go_where_the_line_says_or_to_the_home_directory() {
        let line = |args: &[&str]| CommandLine::try_parse_from(args).unwrap();
        let home = || Some(PathBuf::from("/h"));
        assert_eq!(
            line(&["check", "/out"]).out_dir(home()),
            Some(PathBuf::from("/out"))
        );
        assert_eq!(
            line(&["check"]).out_dir(home()),
            Some(PathBuf::from("/h/zh-package"))
        );
        assert_eq!(line(&["check"]).out_dir(None), None);
    }
}
