# QuickScript integration

ProjectAtlas recognizes `.vbi` and `.vi` by extension as `quickscript`. Detection does not inspect or require filename conventions such as `QF_`, `DCH_`, `APP_`, or `KEY_`.

Normal ProjectAtlas indexing does not execute language servers. The native QuickScript adapter therefore owns only the bounded facts required by the Atlas graph:

- explicit `@ScriptType` and `@Name` metadata produce one script-level symbol;
- `@Description`, `@Event`, `@Trigger`, and `@Shortcut` enrich that symbol without changing its identity;
- `QuickFunction` is the only callable and exported script kind;
- other recognized script kinds are non-callable module symbols;
- uncommented explicit `CALL Name(...)` statements produce call relations; and
- the QuickScript semantic provider resolves those call names to unique indexed QuickFunction declarations.

The adapter deliberately does not implement a QuickScript grammar, legacy-header parser, filename fallback, type inference, diagnostics, formatting, completion, hover, or editor navigation. Those behaviors remain exclusively owned by the native `intouch-language` language server. ProjectAtlas retains only stable repository-index facts and applies its existing incremental generation and relation-resolution pipeline to them.

Repository ignore rules remain authoritative. A project that excludes `.Temp` continues to exclude it from scans and watcher refreshes; QuickScript recognition does not weaken ignore policy.
