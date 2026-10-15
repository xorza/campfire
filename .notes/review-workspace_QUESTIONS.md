# Questions of review-workspace

## Batch 8: the books and engine tags from the declaration walk

The item: "`capabilities/src/books/{book_builder.rs:229-321,book_parts.rs,unit_type_data.rs}`, `areas/src/lib.rs:115`, `projectiles/src/lib.rs:113`, `production/src/lib.rs:188-202`: each module's book declaration carries its build function, `into_books` walks the declarations, and the engine tags are numbered by the walk."

Blocked: this item only. The other Batch 8 items do not need it.

### Q1. The books' build

Now `into_books` adds each of 45 books to `BuiltBooks` by hand. `BookFill` walks the declarations and takes each declared book, and it panics on a declared book that is not built. The books test fails on a built book that no module declares. So the two lists cannot disagree in a match, but a new book needs a line in each.

A build function on each declaration cannot read `BookParts`: it is in the top crate, which names every capability. The data that each book comes from (the unit type sections in `UnitTypeData`, the names, the stat resolution) is also parsed and resolved by the builder.

| Option | What it does | Cost |
| --- | --- | --- |
| **A. Keep the list (recommended)** | `into_books` keeps its list, which the declared-and-built check and the books test hold. | A new book is two lines, in two places, each checked. |
| B. A modular load | Each module declares its unit type section and its book's build through a build context in `declare`. The builder walks the declarations, and `UnitTypeData` becomes the sections that the modules declare. | A redesign of the load: `BookBuilder` (1,174 lines), `UnitTypeData` and the package load check. It needs its own design in `docs/design/` and its own step. |

### Q2. The engine tags' numbers

Now each engine tag is a `const` with its number (`Areas::TAG = EngineTag::new(3, …)`). `Release::engine_tags` walks the declarations and asserts that each number is the tag's place in the walk, at each match load.

| Option | What it does | Cost |
| --- | --- | --- |
| **A. Keep the const numbers (recommended)** | The number stays in the `const`, and the walk checks it. | A new tag's author writes its number, and the walk refuses a wrong one at once. |
| B. Numbers from the walk | A tag's number is its place in the walk, held in a resource. | Each `TAG` stops being a const value: its 27 uses (`Dead::TAG.tag()`, `StatusTags::from([Production::CONSTRUCTING])`, the filters on hot paths) read the number from the world. |
