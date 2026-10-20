use std::num::NonZeroU64;

use campfire_script::ScriptHost;

use super::*;

#[test]
fn a_script_defines_a_hook_only_by_its_name_and_its_count_of_parameters() {
    // `on_resolve` has its 3; `on_hit` takes 4, not 3; `helper` is no hook.
    let source = "fn on_resolve(ctx, unit, target) {} fn on_hit(ctx, unit, target) {} \
                      fn helper() {}";
    let mut host = ScriptHost::new(NonZeroU64::new(1000).unwrap());
    let script = host.compile(source).unwrap();
    let hook = |name, role, param_names| Hook {
        name,
        role,
        param_names,
        param_kinds: &[],
    };
    let resolve = hook("on_resolve", ScriptRole::Action, &["ctx", "unit", "target"]);
    let hit = hook(
        "on_hit",
        ScriptRole::Action,
        &["ctx", "unit", "target", "hit"],
    );
    let timer = hook("on_timer", ScriptRole::Mode, &["ctx", "name", "data"]);
    let mut book = ScriptBook::new([resolve, hit, timer]);
    book.push(host.functions(script));
    // Of an action's hooks, it defines `on_resolve` alone; of the mode's, none.
    assert_eq!(
        book.defines(Some(script), ScriptRole::Action),
        HookSet::from([resolve])
    );
    assert_eq!(book.defines(None, ScriptRole::Action), HookSet::default());
    assert_eq!(
        book.defines(Some(script), ScriptRole::Mode),
        HookSet::default()
    );
    // A second script defines `on_timer`: of the two, the mode's hooks are its `on_timer`, and
    // an action's the first's `on_resolve`; of none, no hook.
    let timed = host.compile("fn on_timer(ctx, name, data) {}").unwrap();
    book.push(host.functions(timed));
    assert_eq!(
        book.defines_any([script, timed], ScriptRole::Mode),
        HookSet::from([timer])
    );
    assert_eq!(
        book.defines_any([timed, script], ScriptRole::Action),
        HookSet::from([resolve])
    );
    assert_eq!(book.defines_any([], ScriptRole::Mode), HookSet::default());
}
