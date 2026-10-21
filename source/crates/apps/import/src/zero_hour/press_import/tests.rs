use super::*;

#[test]
fn the_press_is_the_mouse_s_drag_and_click_time() {
    let mouse = MouseIni {
        drag_tolerance: 4,
        drag_tolerance_ms: 167,
    };
    let keys = keys_file(&mouse);
    assert_eq!(
        keys.press,
        Press {
            drag: 4.0,
            click: 0.167,
        }
    );
    assert_eq!(keys.cast, KeysFile::default().cast);
}
