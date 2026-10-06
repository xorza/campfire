use std::{env, fs, process};

use super::*;

#[test]
fn each_path_is_as_stage_6_names_it_and_one_server_holds_the_directory() {
    let path = env::temp_dir().join(format!("campfire-server-data-{}", process::id()));
    drop(fs::remove_dir_all(&path));
    let data = ServerDir::open(&path).unwrap();
    let id = SessionId::new([0xa7; 32]);
    let hex = "a7".repeat(32);
    assert_eq!(data.path(), path);
    assert_eq!(data.key_file(), path.join("server.nsec"));
    assert_eq!(data.tls_file(), path.join("tls"));
    assert_eq!(data.sessions_dir(), path.join("sessions"));
    assert_eq!(data.session_dir(id), path.join("sessions").join(&hex));
    assert_eq!(data.logs_dir(), path.join("logs"));
    assert_eq!(
        data.published_log(id),
        path.join("logs").join(format!("{hex}.campfire-log"))
    );
    assert!(matches!(ServerDir::open(&path), Err(DataDirError::Locked)));
    drop(data);
    fs::remove_dir_all(&path).unwrap();
}
