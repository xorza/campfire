//! The verifier's binary on the log and the snapshot a match of the test lane mode writes, made
//! by the test itself, so no run of it reads a file another test of the run writes.

use std::path::Path;
use std::process::Command;

use campfire_log::LogEvent;
use campfire_package_disk::PackageDir;
use campfire_protocol::SnapshotFingerprint;
use campfire_runner::SessionWorld;
use campfire_store::internals::TestDir;
use campfire_verifier::{Verified, verify_file};

use crate::session_logs::{Run, checkpointed, encoded};

#[test]
fn the_binary_logs_the_last_state_hash() {
    let mut snapshot = Vec::new();
    let Run { fixed, .. } = checkpointed(&mut snapshot, None, None);
    let log = encoded(fixed.game().session().log());
    // A directory of this run's own, which goes when the test ends, passed or failed.
    let test_dir = TestDir::new();
    let dir = test_dir.root().to_str().unwrap();
    let path = format!("{dir}/headless.log");
    test_dir.write(&path, &log);
    // Logged without color, as standard error is not a terminal here, at `info`, and to no file,
    // whatever the environment says.
    let verifier = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_campfire-verifier"))
            .args(args)
            .env("RUST_LOG", "info")
            .env_remove("CAMPFIRE_LOG")
            .env_remove("CAMPFIRE_LOG_FILTER")
            .output()
            .unwrap()
    };
    // Every package of the workspace: what the verifier holds.
    let packages = PackageDir::workspace("");
    let hash = verify_file(&packages, Path::new(&path), None).unwrap();
    let packages = packages.to_str().unwrap();
    let success = format!("{} file={path} hash={hash}\n", Verified::MESSAGE);
    // With no snapshot, and with the checkpoint's snapshot named by its fingerprint.
    let snapshots = format!("{dir}/snapshots");
    test_dir.create_dir(&snapshots);
    let named = format!("{snapshots}/{}", SnapshotFingerprint::of(&snapshot));
    test_dir.write(&named, &snapshot);
    for args in [
        &["verify", packages, &path][..],
        &["verify", packages, &path, &snapshots],
    ] {
        let output = verifier(args);
        assert!(output.status.success(), "{output:?}");
        let logged = String::from_utf8(output.stderr).unwrap();
        assert!(logged.ends_with(&success), "{logged}");
        assert!(output.stdout.is_empty());
    }
    // A snapshot of other bytes under the checkpoint's name.
    *snapshot.last_mut().unwrap() ^= 1;
    test_dir.write(&named, &snapshot);
    let output = verifier(&["verify", packages, &path, &snapshots]);
    assert_eq!(output.status.code(), Some(1));
    let logged = String::from_utf8(output.stderr).unwrap();
    let refused = format!(
        "the log does not verify file={path} error=a checkpoint's snapshot does not check: the \
         snapshot is not the one its checkpoint names\n"
    );
    assert!(logged.ends_with(&refused), "{logged}");

    let corrupt = format!("{dir}/headless-truncated.log");
    test_dir.write(&corrupt, &log[..log.len() - 1]);
    let output = verifier(&["verify", packages, &corrupt]);
    assert_eq!(output.status.code(), Some(1));
    let logged = String::from_utf8(output.stderr).unwrap();
    let refused = format!(
        "the log does not verify file={corrupt} error=the log does not decode: session log ends inside a field\n"
    );
    assert!(logged.ends_with(&refused), "{logged}");

    assert_eq!(verifier(&[]).status.code(), Some(2));
}
