use super::*;

#[test]
fn a_set_holds_exactly_its_hooks() {
    let set = HookSet::of([Hook::OnTimer, Hook::OnUnitDied]);
    let held: Vec<_> = Hook::ALL
        .into_iter()
        .filter(|&hook| set.contains(hook))
        .collect();
    assert_eq!(held, [Hook::OnTimer, Hook::OnUnitDied]);
    assert!(
        Hook::ALL
            .into_iter()
            .all(|hook| !HookSet::default().contains(hook))
    );
    assert!(Hook::ALL.len() <= 32, "a hook's bit fits a u32");
}
