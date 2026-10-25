use campfire_codec::Json;

use super::*;

#[test]
fn extras_write_and_read_back_and_refuse_runs_out_of_order_or_past_the_last_frame() {
    let extras = AnimationExtras {
        frames: 40,
        rate: 30,
        hidden: vec![
            HiddenFrames {
                node: 2,
                runs: vec![[0, 3], [10, 40]],
            },
            HiddenFrames {
                node: 5,
                runs: vec![[39, 40]],
            },
        ],
    };
    let text = Json::write(&extras);
    assert_eq!(Json::parse::<AnimationExtras>(&text).unwrap(), extras);
    // Extras of no hidden frames write none, and read back.
    let shown = AnimationExtras {
        hidden: Vec::new(),
        ..extras
    };
    let text = Json::write(&shown);
    assert_eq!(text, r#"{"frames":40,"rate":30}"#);
    assert_eq!(Json::parse::<AnimationExtras>(&text).unwrap(), shown);
    let refusal = |text: &str| {
        Json::parse::<AnimationExtras>(text)
            .unwrap_err()
            .to_string()
    };
    let frames = "an animation has no frame or a rate of 0";
    assert_eq!(refusal(r#"{"frames": 0, "rate": 30}"#), frames);
    assert_eq!(refusal(r#"{"frames": 4, "rate": 0}"#), frames);
    let nodes = "a node's hidden frames are not in order";
    let twice = r#"{"frames": 4, "rate": 30, "hidden": [{"node": 1, "runs": []}, {"node": 1, "runs": []}]}"#;
    assert_eq!(refusal(twice), nodes);
    let runs = "a run of hidden frames is empty, past the last frame or out of order";
    for bad in [
        "[[2, 2]]",
        "[[3, 5]]",
        "[[2, 4], [0, 1]]",
        "[[0, 2], [2, 4]]",
    ] {
        let text =
            format!(r#"{{"frames": 4, "rate": 30, "hidden": [{{"node": 1, "runs": {bad}}}]}}"#);
        assert_eq!(refusal(&text), runs, "{bad}");
    }
}
