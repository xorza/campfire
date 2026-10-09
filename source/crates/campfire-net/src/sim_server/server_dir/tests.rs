use campfire_protocol::SessionId;
use campfire_store::SecretFile;
use tempfile::TempDir;

use super::*;

#[test]
fn each_path_is_as_stage_6_names_it_and_one_server_holds_the_directory() {
    let scratch = TempDir::new().unwrap();
    let path = scratch.path().join("data");
    let data = ServerDir::open(&path).unwrap();
    let id = SessionId::new([0xa7; 32]);
    let hex = "a7".repeat(32);
    let layout = data.layout();
    assert_eq!(*layout, ServerLayout::at(path.clone()));
    assert_eq!(layout.path(), path);
    assert_eq!(layout.key_file(), SecretFile::at(path.join("server.nsec")));
    assert_eq!(layout.tls_file(), SecretFile::at(path.join("tls")));
    assert_eq!(layout.sessions_dir(), path.join("sessions"));
    assert_eq!(layout.session_dir(id), path.join("sessions").join(&hex));
    assert_eq!(layout.logs_dir(), path.join("logs"));
    assert_eq!(
        layout.published_log(id),
        path.join("logs").join(format!("{hex}.campfire-log"))
    );
    assert!(matches!(ServerDir::open(&path), Err(DataDirError::Locked)));
    drop(data);
}
