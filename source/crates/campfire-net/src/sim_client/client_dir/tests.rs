use tempfile::TempDir;

use super::*;

#[test]
fn each_path_is_as_stage_6_names_it_and_a_second_client_is_refused() {
    let scratch = TempDir::new().unwrap();
    let path = scratch.path().join("data");
    let data = ClientDir::open(&path).unwrap();
    let id = SessionId::new([0xa7; 32]);
    assert_eq!(data.receipts_dir(), path.join("receipts"));
    assert_eq!(
        data.receipt_file(id),
        path.join("receipts")
            .join(format!("{}.receipt", "a7".repeat(32)))
    );
    assert_eq!(data.local_server_dir(), path.join("server"));
    // A second client on the directory is refused, and its local server's directory, a data
    // directory of its own, is not held by the client's lock.
    assert!(matches!(ClientDir::open(&path), Err(DataDirError::Locked)));
    let local = DataDir::open(&data.local_server_dir()).unwrap();
    drop((local, data));
}
