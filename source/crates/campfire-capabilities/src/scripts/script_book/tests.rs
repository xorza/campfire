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
    let mut book = ScriptBook::default();
    book.push(host.functions(script));
    // Of an action's hooks, it defines `on_resolve` alone; of the mode's, none.
    assert_eq!(
        book.defines(Some(script), ScriptRole::Action),
        HookSet::of([Hook::OnResolve])
    );
    assert_eq!(book.defines(None, ScriptRole::Action), HookSet::default());
    assert_eq!(
        book.defines(Some(script), ScriptRole::Mode),
        HookSet::default()
    );
}
