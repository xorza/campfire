use std::env;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::error::CheckError;
use crate::target_name::TargetName;

/// The executables the check runs, as cargo built them.
#[derive(Debug)]
pub(crate) struct Binaries {
    pub(crate) server: PathBuf,
    pub(crate) client: PathBuf,
    pub(crate) verifier: PathBuf,
}

/// A line of cargo's JSON output: the check reads only the artifacts it builds.
#[derive(Debug, Deserialize)]
#[serde(tag = "reason", rename_all = "kebab-case")]
enum Message {
    CompilerArtifact {
        target: Target,
        executable: Option<PathBuf>,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct Target {
    name: TargetName,
}

impl Binaries {
    /// Builds the server, client and verifier with `cargo`, in its default profile, and finds
    /// their executables.
    pub(crate) fn build(cargo: &OsStr) -> Result<Binaries, CheckError> {
        let mut command = Command::new(cargo);
        // `cargo run` gives the check its package's variables; a build script that tracks one,
        // as `ring`'s tracks `CARGO_MANIFEST_DIR`, would rebuild on every run with them.
        for (name, _) in env::vars_os() {
            let package = name.to_str().is_some_and(|name| {
                name.starts_with("CARGO_PKG_") || name.starts_with("CARGO_MANIFEST_")
            });
            if package {
                command.env_remove(name);
            }
        }
        command.arg("build");
        for target in TargetName::RUN {
            command.args(["-p", target.name()]);
        }
        let output = command
            .arg("--message-format=json-render-diagnostics")
            .stderr(Stdio::inherit())
            .output()
            .map_err(CheckError::CargoStart)?;
        if !output.status.success() {
            return Err(CheckError::Build);
        }
        let mut built = TargetName::RUN.map(|target| (target, None));
        for line in output.stdout.split(|&byte| byte == b'\n') {
            let Ok(Message::CompilerArtifact {
                target,
                executable: Some(executable),
            }) = serde_json::from_slice(line)
            else {
                continue;
            };
            if let Some((_, path)) = built.iter_mut().find(|(name, _)| *name == target.name) {
                *path = Some(executable);
            }
        }
        let [server, client, verifier] =
            built.map(|(target, path)| path.ok_or(CheckError::NoExecutable(target)));
        Ok(Binaries {
            server: server?,
            client: client?,
            verifier: verifier?,
        })
    }
}
