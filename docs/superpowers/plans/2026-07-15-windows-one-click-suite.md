# Windows One-Click MEM Suite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish a single Windows x64 `Setup.exe` that installs CC Switch MEM and configures the pinned Claude-Mem Local fork for loopback, real-time model following.

**Architecture:** CC Switch remains the outer Tauri application and NSIS installer. Its release build embeds a pinned Node runtime, a built Claude-Mem package tree, and a Node-stdlib orchestration helper. Claude-Mem's own CLI remains responsible for plugin files, hooks, Bun/uv, worker lifecycle, and provider settings.

**Tech Stack:** Tauri 2, Rust, React/Vite, NSIS, Node.js 24 LTS, npm, Bun, TypeScript, Vitest, GitHub Actions.

## Global Constraints

- Windows 10/11 x64 is the first supported suite platform.
- CC Switch version is `3.17.0-mem.1`; Claude-Mem Local version is `13.11.0-local.4`.
- The suite must use `satan9394/cc-switch_mem` and `satan9394/claude-mem_local`, never the same-named upstream npm package.
- Claude-Mem uses `cc-switch-auto` with `follow-session`; no fixed model or provider fallback is allowed.
- Ordinary Claude Code model names must remain unchanged; only Claude-Mem usage is labelled `MEM`.
- Installer logs must not contain prompts, transcripts, provider secrets, memory content, or environment-variable values.
- No new runtime dependency or installer framework may be added.

---

### Task 1: Claude-Mem non-interactive CC Switch provider

**Files:**
- Modify: sibling `claude-mem_local/src/npx-cli/index.ts`
- Modify: sibling `claude-mem_local/src/npx-cli/commands/install.ts`
- Test: sibling `claude-mem_local/tests/install-non-tty.test.ts`
- Test: sibling `claude-mem_local/tests/worker/providers/provider-config.test.ts`

**Interfaces:**
- Consumes: `createDefaultProviderConfig()` and `serializeProviderConfig()` from `src/services/worker/providers/provider-config.ts`.
- Produces: `InstallOptions.provider` value `cc-switch` and persisted `providerMode: "cc-switch-auto"`, `modelPolicy: "follow-session"`.

- [ ] **Step 1: Write failing installer tests**

Add source-contract assertions and a provider-config assertion:

```ts
expect(indexSource).toContain("provider !== 'cc-switch'");
expect(installSource).toContain("provider?: 'claude' | 'gemini' | 'openrouter' | 'cc-switch'");

const config = createDefaultProviderConfig();
config.providerMode = 'cc-switch-auto';
config.modelPolicy = 'follow-session';
expect(parseProviderConfig(serializeProviderConfig(config), 'claude')).toMatchObject({
  providerMode: 'cc-switch-auto',
  modelPolicy: 'follow-session',
});
```

- [ ] **Step 2: Verify RED**

Run:

```powershell
bun test tests/install-non-tty.test.ts tests/worker/providers/provider-config.test.ts
```

Expected: the installer source assertion fails because `cc-switch` is not accepted.

- [ ] **Step 3: Implement the minimal provider branch**

Extend the CLI type/parser and persist the existing provider config:

```ts
type ProviderId = 'claude' | 'gemini' | 'openrouter' | 'cc-switch';

if (options.provider === 'cc-switch') {
  const config = createDefaultProviderConfig();
  config.providerMode = 'cc-switch-auto';
  config.modelPolicy = 'follow-session';
  const wrote = mergeSettings({
    CLAUDE_MEM_PROVIDER: config.legacyProvider,
    CLAUDE_MEM_PROVIDER_CONFIG: serializeProviderConfig(config),
  });
  if (wrote) log.info('Configured loopback CC Switch with real-time session model following.');
  return 'cc-switch';
}
```

Skip `promptClaudeModel()` for `cc-switch`.

- [ ] **Step 4: Verify GREEN and regression scope**

Run:

```powershell
bun test tests/install-non-tty.test.ts tests/worker/providers/provider-config.test.ts tests/worker/providers/cc-switch-provider.test.ts tests/integration/cc-switch-provider-e2e.test.ts
npm run typecheck
npm run build
```

Expected: all commands exit 0.

- [ ] **Step 5: Commit**

```powershell
git add src/npx-cli/index.ts src/npx-cli/commands/install.ts tests/install-non-tty.test.ts tests/worker/providers/provider-config.test.ts
git commit -m "feat(installer): configure CC Switch non-interactively"
```

### Task 2: Claude-Mem release package and standalone installer assets

**Files:**
- Create: sibling `claude-mem_local/scripts/release/build-local-assets.mjs`
- Create: sibling `claude-mem_local/tests/infrastructure/local-release-assets.test.ts`
- Create: sibling `claude-mem_local/install/windows/install-claude-mem-local.ps1`
- Create: sibling `claude-mem_local/.github/workflows/release-local.yml`
- Modify: sibling `claude-mem_local/package.json`
- Modify: sibling `claude-mem_local/README.md`
- Create: sibling `claude-mem_local/docs/releases/v13.11.0-local.4.md`

**Interfaces:**
- Produces: `claude-mem-local-13.11.0-local.4.tgz`, `claude-mem-local-13.11.0-local.4.zip`, `install-claude-mem-local.ps1`, and `SHA256SUMS.txt`.
- The ZIP root is a ready-to-run package tree containing `dist/npx-cli/index.js`, `plugin/`, and `package.json`.

- [ ] **Step 1: Write failing release-asset test**

```ts
test('local release assets are fork-pinned and one-click capable', () => {
  const script = readFileSync('scripts/release/build-local-assets.mjs', 'utf8');
  const installer = readFileSync('install/windows/install-claude-mem-local.ps1', 'utf8');
  expect(script).toContain('npm pack');
  expect(script).toContain('SHA256SUMS.txt');
  expect(installer).toContain('--provider cc-switch');
  expect(installer).toContain('--runtime worker');
  expect(installer).toContain('satan9394/claude-mem_local');
  expect(installer).not.toContain('npx claude-mem');
});
```

- [ ] **Step 2: Verify RED**

Run `bun test tests/infrastructure/local-release-assets.test.ts`.

Expected: FAIL because the asset builder and Windows installer do not exist.

- [ ] **Step 3: Implement asset builder and PowerShell entrypoint**

The Node builder uses `spawnSync('npm', ['pack', '--json', '--pack-destination', outDir])`, stages the package tree with `tar -xzf`, creates the ZIP with PowerShell `Compress-Archive` on Windows, and writes hashes with `createHash('sha256')`.

The standalone installer downloads the exact release ZIP, verifies its SHA-256 from the same release, expands under `%LOCALAPPDATA%\claude-mem-local\versions\13.11.0-local.4`, and invokes:

```powershell
& $nodeExe "$packageRoot\dist\npx-cli\index.js" install --ide claude-code --provider cc-switch --runtime worker
```

- [ ] **Step 4: Add release workflow and user-facing instructions**

The workflow triggers only on `v*-local.*` tags, runs the existing test/build gates, invokes `node scripts/release/build-local-assets.mjs`, uploads the four assets, and generates release notes from `docs/releases/v13.11.0-local.4.md`.

- [ ] **Step 5: Verify assets locally**

Run:

```powershell
npm run build
node scripts/release/build-local-assets.mjs
Get-FileHash release-assets\* -Algorithm SHA256
bun test tests/infrastructure/local-release-assets.test.ts
npm run typecheck
```

Expected: the ZIP and TGZ exist, recorded hashes match `Get-FileHash`, and tests exit 0.

- [ ] **Step 6: Commit**

```powershell
git add scripts/release tests/infrastructure/local-release-assets.test.ts install/windows .github/workflows/release-local.yml package.json README.md docs/releases/v13.11.0-local.4.md
git commit -m "feat(release): build Claude-Mem Local installer assets"
```

### Task 3: CC Switch suite lock and preparation pipeline

**Files:**
- Create: `suite-lock.json`
- Create: `scripts/suite/validate-lock.mjs`
- Create: `scripts/suite/prepare-assets.ps1`
- Create: `tests/suite/suiteManifest.test.ts`
- Modify: `package.json`

**Interfaces:**
- Produces: `src-tauri/resources/suite/node/`, `src-tauri/resources/suite/claude-mem/`, and a normalized `manifest.json`.
- Pins Node `24.18.0` Windows x64 ZIP with SHA-256 `0ae68406b42d7725661da979b1403ec9926da205c6770827f33aac9d8f26e821`.

- [ ] **Step 1: Write failing manifest test**

```ts
test('pins only the maintained fork and exact runtime hashes', async () => {
  const lock = JSON.parse(readFileSync('suite-lock.json', 'utf8'));
  expect(lock.ccSwitch).toMatchObject({ version: '3.17.0-mem.1', repository: 'satan9394/cc-switch_mem' });
  expect(lock.claudeMem).toMatchObject({ version: '13.11.0-local.4', repository: 'satan9394/claude-mem_local' });
  expect(lock.node.sha256).toMatch(/^[a-f0-9]{64}$/);
  expect(lock.node.url).toBe('https://nodejs.org/dist/v24.18.0/node-v24.18.0-win-x64.zip');
});
```

- [ ] **Step 2: Verify RED**

Run `pnpm test:unit -- tests/suite/suiteManifest.test.ts`.

Expected: FAIL because `suite-lock.json` does not exist.

- [ ] **Step 3: Implement lock validation and asset preparation**

`validate-lock.mjs` rejects non-HTTPS runtime URLs, non-fork repositories, moving refs such as `main`, version disagreement, and malformed SHA-256 values. `prepare-assets.ps1` accepts `-ClaudeMemSource`, runs the sibling build and asset command, verifies the package version, downloads and verifies Node, and stages only the package tree and runtime needed by the installer.

- [ ] **Step 4: Verify GREEN**

Run:

```powershell
pnpm test:unit -- tests/suite/suiteManifest.test.ts
node scripts/suite/validate-lock.mjs
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/suite/prepare-assets.ps1 -ClaudeMemSource ..\..\..\claude-mem_loca\.worktrees\windows-one-click-suite
```

Expected: all commands exit 0 and the staged package reports `13.11.0-local.4`.

- [ ] **Step 5: Commit**

```powershell
git add suite-lock.json scripts/suite tests/suite package.json
git commit -m "build: pin one-click suite components"
```

### Task 4: Tauri NSIS suite installer and rollback helper

**Files:**
- Create: `scripts/suite/install-suite.mjs`
- Create: `tests/suite/installSuite.test.ts`
- Create: `src-tauri/windows/suite-hooks.nsh`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/tauri.windows.conf.json`

**Interfaces:**
- Consumes: installed resources under `$INSTDIR/resources/suite`.
- Produces: a per-user NSIS setup executable and sanitized `%LOCALAPPDATA%\claude-mem-local\logs\suite-install.log`.

- [ ] **Step 1: Write failing orchestration tests**

```ts
test('builds a secret-free child invocation', () => {
  expect(buildInstallArgs('C:\\suite\\claude-mem')).toEqual([
    'C:\\suite\\claude-mem\\dist\\npx-cli\\index.js',
    'install', '--ide', 'claude-code', '--provider', 'cc-switch', '--runtime', 'worker',
  ]);
});

test('redacts token-like values from the suite log', () => {
  expect(sanitizeLog('ANTHROPIC_API_KEY=secret-value')).not.toContain('secret-value');
});
```

- [ ] **Step 2: Verify RED**

Run `pnpm test:unit -- tests/suite/installSuite.test.ts`.

Expected: FAIL because `install-suite.mjs` does not exist.

- [ ] **Step 3: Implement the Node-stdlib helper**

Export `buildInstallArgs`, `sanitizeLog`, `backupConfiguration`, `restoreConfiguration`, and `runSuiteInstall`. Back up only settings files that the Claude-Mem CLI may modify, spawn the embedded Node binary with an explicit package entrypoint, run `doctor`, restore backups on a non-zero result, and never serialize the child environment.

- [ ] **Step 4: Add NSIS hook and resource mapping**

The post-install hook uses `nsExec::ExecToStack` with:

```nsh
'"$INSTDIR\resources\suite\node\node.exe" "$INSTDIR\resources\suite\install-suite.mjs" --root "$INSTDIR\resources\suite"'
```

On failure it displays the sanitized log location and keeps CC Switch installed. Configure `bundle.targets` to include `nsis`, `bundle.resources` to include the staged suite tree, `nsis.installMode` to `currentUser`, and `nsis.installerHooks` to `./windows/suite-hooks.nsh` in the Windows release overlay.

- [ ] **Step 5: Verify GREEN and build**

Run:

```powershell
pnpm test:unit -- tests/suite/installSuite.test.ts tests/suite/suiteManifest.test.ts
pnpm typecheck
pnpm tauri build --bundles nsis
```

Expected: tests pass and `src-tauri/target/release/bundle/nsis/*-setup.exe` exists.

- [ ] **Step 6: Commit**

```powershell
git add scripts/suite/install-suite.mjs tests/suite/installSuite.test.ts src-tauri/windows/suite-hooks.nsh src-tauri/tauri.conf.json src-tauri/tauri.windows.conf.json
git commit -m "feat(installer): bundle Claude-Mem with CC Switch"
```

### Task 5: Fork-safe updater and automated release assets

**Files:**
- Modify: `package.json`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `.github/workflows/release.yml`
- Modify: `.github/workflows/ci.yml`
- Modify: `README.md`
- Create: `docs/release-notes/v3.17.0-mem.1-zh.md`

**Interfaces:**
- Produces: `CC-Switch-MEM-Suite-v3.17.0-mem.1-Windows-x64-Setup.exe`, CC Switch-only MSI, portable ZIP, updater artifacts, and `SHA256SUMS.txt`.
- Updater endpoint: `https://github.com/satan9394/cc-switch_mem/releases/latest/download/latest.json`.

- [ ] **Step 1: Extend manifest tests with release invariants**

Assert that all three version files agree, the updater endpoint names the fork, the release workflow uploads the suite setup and checksums, and release notes prominently mention Claude-Mem Local, local-only memory, provider egress, and unsigned SmartScreen behavior.

- [ ] **Step 2: Verify RED**

Run `pnpm test:unit -- tests/suite/suiteManifest.test.ts`.

Expected: FAIL on version, updater, workflow, and documentation assertions.

- [ ] **Step 3: Implement version and workflow changes**

Set package/Tauri/Cargo versions to `3.17.0-mem.1`, switch the updater endpoint, add a Windows x64 suite build path that checks out `satan9394/claude-mem_local` at `v13.11.0-local.4`, prepares embedded assets, builds NSIS, renames the setup artifact, generates hashes, and uploads all files. Keep upstream multi-platform jobs intact for CC Switch-only artifacts.

- [ ] **Step 4: Add CI validation**

Add a Windows job that checks out both PR branches, prepares suite assets, builds NSIS without publishing, installs with `/S`, verifies the installed executable and sanitized suite log, then uninstalls CC Switch while asserting the Claude-Mem data directory remains.

- [ ] **Step 5: Verify GREEN**

Run:

```powershell
pnpm test:unit -- tests/suite/suiteManifest.test.ts tests/suite/installSuite.test.ts
pnpm typecheck
pnpm format:check
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: all commands exit 0.

- [ ] **Step 6: Commit**

```powershell
git add package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json .github/workflows/release.yml .github/workflows/ci.yml README.md docs/release-notes/v3.17.0-mem.1-zh.md tests/suite/suiteManifest.test.ts
git commit -m "ci(release): publish Windows MEM suite"
```

### Task 6: Clean-room acceptance and publication preparation

**Files:**
- Create: `scripts/suite/verify-installed-suite.ps1`
- Create: `docs/releases/windows-one-click-verification.md`
- Modify: sibling `claude-mem_local/docs/releases/v13.11.0-local.4.md`

**Interfaces:**
- Consumes: built NSIS installer, live CC Switch loopback proxy, installed Claude-Mem worker.
- Produces: reproducible verification report without prompt or secret content.

- [ ] **Step 1: Implement the acceptance verifier**

The script verifies file versions and hashes, starts CC Switch, waits for loopback health, runs Claude-Mem `doctor`, sends a fixed synthetic normal request and a fixed synthetic MEM request under the same session, queries usage metadata, and asserts same resolved model plus `proxy`/`MEM` attribution. It also sends a missing-session request and asserts local 409.

- [ ] **Step 2: Run complete fresh verification**

Run:

```powershell
pnpm install --frozen-lockfile
pnpm typecheck
pnpm format:check
pnpm test:unit
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml

Push-Location ..\..\..\claude-mem_loca\.worktrees\windows-one-click-suite
npm install --no-audit --no-fund
npm run typecheck
npm run build
bun test
node scripts/release/build-local-assets.mjs
Pop-Location

powershell -NoProfile -ExecutionPolicy Bypass -File scripts/suite/verify-installed-suite.ps1
```

Expected: zero test failures, successful installer and worker health, matching model metadata, correct `MEM` attribution, and local 409 fail-closed behavior.

- [ ] **Step 3: Audit artifacts**

Confirm every release file has a recorded SHA-256, no release archive contains `.env`, settings databases, prompts, API keys, private updater keys, or local absolute paths, and `git diff --check` plus both `git status -sb` outputs are clean after commits.

- [ ] **Step 4: Commit verification documentation**

```powershell
git add scripts/suite/verify-installed-suite.ps1 docs/releases/windows-one-click-verification.md
git commit -m "test: verify Windows one-click MEM suite"
```

Commit the companion Claude-Mem release-note update separately in its repository.

- [ ] **Step 5: Prepare GitHub publication without publishing**

Push neither branch and create no tag until the final publication confirmation. Record the intended PR titles, release tags, asset names, checksums, and required repository secrets locally so the publish step is a deterministic final operation.
