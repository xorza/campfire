use super::*;

#[test]
fn a_set_holds_exactly_its_hooks() {
    let hook = |name, role| Hook {
        name,
        role,
        param_names: &["ctx"],
    };
    let timer = hook("on_timer", ScriptRole::Mode);
    let died = hook("on_unit_death", ScriptRole::Mode);
    let think = hook("on_think", ScriptRole::Ai);
    let set = HookSet::from([died, think, timer, timer]);
    let held: Vec<_> = [timer, died, think]
        .into_iter()
        .filter(|&hook| set.contains(hook))
        .collect();
    assert_eq!(held, [timer, died, think]);
    assert!(!HookSet::default().contains(timer));
    // Each once, by name.
    assert_eq!(set.iter().collect::<Vec<_>>(), [think, timer, died]);
    assert_eq!((&set).into_iter().len(), 3);
    // Of the mode's role, the two of its hooks; of the AI's, `on_think` alone.
    assert_eq!(set.of_role(ScriptRole::Mode), HookSet::from([timer, died]));
    assert_eq!(set.of_role(ScriptRole::Ai), HookSet::from([think]));
    assert!(!set.of_role(ScriptRole::Ai).contains(timer));
}
