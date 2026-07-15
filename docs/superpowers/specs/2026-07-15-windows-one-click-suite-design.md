# Windows One-Click MEM Suite Design

**Status:** Approved by the user on 2026-07-15

## Goal

Ship a public Windows 10/11 x64 installer that installs the maintained `satan9394/cc-switch_mem` and `satan9394/claude-mem_local` pair in one guided run. A user must not need to clone either repository, build source, select a fixed Claude-Mem model, or manually edit Claude Code configuration.

## Chosen approach

Extend the existing CC Switch Tauri 2 Windows bundle with an NSIS suite target. Tauri already owns the desktop application, installer identity, per-user install directory, WebView2 handling, and updater. The suite target adds a post-install helper and pinned Claude-Mem payload instead of introducing a third application or an unrelated installer framework.

The normal CC Switch MSI and portable ZIP remain available. The suite `Setup.exe` is the recommended download for new users.

## Release pair

- CC Switch MEM: `3.17.0-mem.1`, tag `v3.17.0-mem.1`.
- Claude-Mem Local: `13.11.0-local.4`, tag `v13.11.0-local.4`.
- The CC Switch release workflow checks out the exact Claude-Mem tag recorded in a committed suite lock file. It builds the Claude-Mem package rather than resolving the upstream `claude-mem` npm name.
- Every public binary and package receives a SHA-256 checksum. Tauri updater artifacts also retain their existing minisign signatures.

## Components

### CC Switch suite installer

The release workflow builds the existing Tauri application as:

- a per-user NSIS `Setup.exe` for the complete suite;
- the existing MSI for CC Switch-only installation;
- the existing portable ZIP.

The NSIS bundle includes:

- a versioned Claude-Mem Local package produced by `npm pack` from the pinned fork tag;
- a small PowerShell orchestration script;
- a suite manifest containing component versions, source repositories, and SHA-256 hashes.

The fork changes its updater endpoint from `farion1231/cc-switch` to `satan9394/cc-switch_mem`, so an upstream official update cannot silently replace MEM-specific behavior.

### Claude-Mem installer contract

The existing Claude-Mem CLI remains the single owner of plugin layout, runtime dependencies, Claude Code hook registration, worker lifecycle, repair, and uninstall. It gains a non-interactive `--provider cc-switch` option that writes the existing versioned provider configuration with:

- `providerMode: "cc-switch-auto"`;
- `modelPolicy: "follow-session"`;
- loopback-only discovery;
- no fixed fallback model or provider.

This option does not rename or modify ordinary Claude Code models. Only Claude-Mem proxy traffic receives the existing `MEM` attribution marker.

### Runtime prerequisites

The suite helper performs deterministic preflight checks before changing Claude configuration:

1. Verify Windows x64 and the embedded suite manifest.
2. Verify the packaged Claude-Mem archive hash.
3. Locate Node.js 20.12 or newer. If absent, install the pinned official Node LTS per-user package used by the release workflow.
4. Invoke the packaged Claude-Mem CLI with `install --ide claude-code --provider cc-switch --runtime worker`.
5. Let the existing CLI install or verify Bun and uv using its current audited paths.
6. Run `doctor` and record the result before reporting success.

The first version may require network access for missing Node, Bun, and uv. The installer must disclose each official destination in its progress log. Model prompts are not sent during installation.

## Data and control flow

```text
GitHub Release Setup.exe
  -> verify embedded manifest and Claude-Mem package
  -> install CC Switch per-user
  -> run Claude-Mem Local CLI from pinned package
  -> register Claude Code hooks and local worker
  -> activate cc-switch-auto + follow-session
  -> run doctor
  -> launch CC Switch

Claude Code normal request
  -> CC Switch records session model without renaming it

Claude-Mem request
  -> loopback CC Switch with MEM marker + session identity
  -> same resolved session model
  -> selected CC Switch provider
```

Cloud Sync, telemetry, implicit external upload, fixed-model fallback, and fail-open routing remain disabled.

## Failure and rollback

- Before writing Claude Code or Claude-Mem settings, the helper creates a timestamped backup under the user's Claude-Mem data directory.
- Hash, architecture, or manifest failures stop before configuration changes.
- If Claude-Mem installation or `doctor` fails, the helper restores the configuration backup, leaves the diagnostic log, and returns a non-zero exit code.
- CC Switch remains installed if its own installation succeeded, because it is independently usable; the final installer page clearly reports that Claude-Mem setup failed and provides a one-click retry command.
- Missing CC Switch session state continues to return local HTTP 409 and never consumes upstream tokens.
- Uninstalling CC Switch does not delete Claude-Mem databases. Claude-Mem removal remains an explicit separate action to protect user data.

## Logging and privacy

The installer log contains component versions, hashes, exit codes, and sanitized paths. It must not contain prompts, provider secrets, environment-variable values, Claude transcripts, or memory database content.

Before provider activation, allowed network destinations are limited to GitHub release assets and the official Node, Bun, and uv distribution endpoints already used by the installers. After activation, model traffic follows only the provider selected in CC Switch.

## CI and release gates

### Claude-Mem Local

- Test the new `cc-switch` CLI option red-green before implementation.
- Run focused installer/provider tests, full typecheck, production build, package allowlist check, and `npm pack --dry-run`.
- Upload the `.tgz`, checksum file, and standalone PowerShell installer as release assets.

### CC Switch MEM

- Test the suite manifest and artifact naming before implementation.
- Run frontend typecheck/unit tests and Rust proxy tests.
- Build MSI, portable ZIP, NSIS suite installer, updater metadata, and checksums on Windows x64.
- Run a silent installer smoke test on a clean GitHub-hosted Windows runner.

### End-to-end acceptance

On a clean Windows user profile:

1. Install with one `Setup.exe` invocation.
2. Confirm CC Switch launches without `localhost` or blank-page failures.
3. Confirm Claude-Mem `doctor` and worker health pass.
4. Send one normal Claude-compatible request and one Claude-Mem request in the same synthetic session.
5. Confirm both resolve to the same selected model, only the Claude-Mem row is labelled `MEM`, and the normal model name is unchanged.
6. Confirm missing session state returns local 409 with no upstream request.
7. Uninstall CC Switch and confirm Claude-Mem data is preserved.

## Distribution and trust

The release page prominently labels this as an independent local-security fork and links both source commits. It exposes checksums and GitHub Actions provenance. If no trusted Authenticode certificate is configured, the release notes must explicitly disclose that Windows SmartScreen may show an unknown-publisher warning; checksums and Tauri updater signatures do not substitute for Authenticode trust.

Publication remains a separate confirmation gate after all artifacts and checks pass.
