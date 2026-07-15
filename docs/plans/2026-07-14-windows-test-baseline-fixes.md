# Windows test baseline fixes

## Goal

Make the CC Switch Windows library test suite deterministic without changing production routing, provider selection, model names, or Claude-Mem attribution.

## Root causes and minimal fixes

1. Build `sqlite_home` test input with TOML serialization so Windows backslashes are valid TOML escapes. Keep production parsing strict.
2. Align the six Windows Codex upgrade assertions with the current safety policy: use the anchored package manager directly and do not call `codex update`, which can report success without repairing a missing optional binary.
3. Configure the Claude Desktop takeover test with port `0`, as neighboring proxy tests already do, so the OS selects an isolated port instead of competing with the installed CC Switch listener.

## Verification

Run the three affected test groups first, then `cargo fmt --check`, then the complete `cargo test --manifest-path src-tauri/Cargo.toml --lib` suite. Re-run the MEM attribution tests afterward to prove the baseline repair did not alter attribution behavior.
