# Windows one-click suite verification

The release gate uses the exact public NSIS candidate and never writes into the maintainer's real Claude, Codex, or Claude-Mem directories.

```powershell
./scripts/suite/verify-installed-suite.ps1 `
  -Installer "src-tauri/target/release/bundle/nsis/CC Switch MEM Suite_3.17.0-mem.1_x64-setup.exe" `
  -RunIsolatedInstall
```

The verifier checks:

- the NSIS archive extracts without corruption;
- embedded CC Switch, Claude-Mem Local, and Node versions match `suite-lock.json`;
- the embedded Node runtime is exactly the pinned Windows x64 version;
- the installer persists `cc-switch-auto`, `follow-session`, and an empty fixed model;
- the worker is started and answers its loopback health endpoint;
- the isolated worker is stopped and the temporary user directory is removed.
- the release build emits a Tauri updater signature beside the Setup EXE.

The normal Claude Code model name is never modified. Runtime integration tests separately assert that only MEM traffic gets MEM attribution, a session model is followed in real time, and a missing session fails locally without upstream contact.

Local clean-room validation on 2026-07-15 produced a 36.96 MB NSIS setup, completed the embedded installer, and returned a healthy worker on `127.0.0.1:37777`.
