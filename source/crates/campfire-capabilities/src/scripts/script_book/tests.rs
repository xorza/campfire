use campfire_script::ScriptHost;
use std::num::NonZeroU64;

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
    let hooks = [Hook::OnResolve, Hook::OnHit, Hook::OnEnd];
    assert_eq!(
        book.defines(Some(script), &hooks),
        HookSet::of([Hook::OnResolve])
    );
    assert_eq!(book.defines(None, &hooks), HookSet::default());
    assert_eq!(
        book.defines(Some(script), &[Hook::OnHit]),
        HookSet::default()
    );
}
