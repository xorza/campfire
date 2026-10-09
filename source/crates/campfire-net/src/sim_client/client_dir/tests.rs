use campfire_protocol::SessionId;
use campfire_store::Scratch;

use super::*;

#[test]
fn each_path_is_as_stage_6_names_it_and_a_second_client_is_refused() {
    let scratch = Scratch::new();
    let path = scratch.path("data");
    let data = ClientDir::open(&path).unwrap();
    let id = SessionId::new([0xa7; 32]);
    let layout = data.layout();
    assert_eq!(*layout, ClientLayout::at(path.clone()));
    assert_eq!(layout.receipts_dir(), path.join("receipts"));
    assert_eq!(
        layout.receipt_file(id),
        path.join("receipts")
            .join(format!("{}.receipt", "a7".repeat(32)))
    );
    assert_eq!(layout.local_server_dir(), path.join("server"));
    // A second client on the directory is refused, and its local server's directory, a data
    // directory of its own, is not held by the client's lock.
    assert!(matches!(
        ClientDir::open(&path),
        Err(PathError {
            error: DataDirError::Locked,
            ..
        })
    ));
    let local = DataDir::open(&layout.local_server_dir()).unwrap();
    drop((local, data));
}
