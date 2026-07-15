import { spawnSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { homedir, tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

export function buildInstallArgs(packageRoot) {
  return [
    join(packageRoot, "dist", "npx-cli", "index.js"),
    "install",
    "--ide",
    "claude-code",
    "--provider",
    "cc-switch",
    "--runtime",
    "worker",
  ];
}

export function sanitizeLog(value) {
  return String(value)
    .replace(/((?:api[_-]?key|token|secret|password)\s*[:=]\s*)[^\s]+/gi, "$1[REDACTED]")
    .replace(/(authorization\s*:\s*(?:bearer\s+)?)[^\s]+/gi, "$1[REDACTED]")
    .replace(/(sk-[A-Za-z0-9_-]{8})[A-Za-z0-9_-]+/g, "$1[REDACTED]");
}

export function backupConfiguration(files = [
  join(homedir(), ".claude-mem", "settings.json"),
  join(homedir(), ".claude", "settings.json"),
]) {
  const directory = mkdtempSync(join(tmpdir(), "cc-switch-mem-backup-"));
  const entries = [];
  for (const [index, source] of files.entries()) {
    if (!existsSync(source)) continue;
    const backup = join(directory, `${index}-${basename(source)}`);
    copyFileSync(source, backup);
    entries.push({ source, backup });
  }
  return { directory, entries };
}

export function restoreConfiguration(backup) {
  for (const { source, backup: backupFile } of backup.entries) {
    mkdirSync(dirname(source), { recursive: true });
    copyFileSync(backupFile, source);
  }
}

function run(nodeExe, args) {
  return spawnSync(nodeExe, args, {
    encoding: "utf8",
    windowsHide: true,
    shell: false,
    env: process.env,
  });
}

export function runSuiteInstall(root) {
  const nodeExe = join(root, "node", "node.exe");
  const packageRoot = join(root, "claude-mem");
  const logPath = join(process.env.LOCALAPPDATA ?? homedir(), "claude-mem-local", "logs", "suite-install.log");
  const backup = backupConfiguration();
  const messages = [`suite install started`, `suite root: ${root}`];
  let exitCode = 1;

  try {
    if (!existsSync(nodeExe)) throw new Error("Embedded Node runtime is missing");
    const install = run(nodeExe, buildInstallArgs(packageRoot));
    messages.push(install.stdout ?? "", install.stderr ?? "");
    if (install.error) throw install.error;
    if (install.status !== 0) throw new Error(`Claude-Mem install exited with ${install.status}`);

    const doctor = run(nodeExe, [join(packageRoot, "dist", "npx-cli", "index.js"), "doctor"]);
    messages.push(doctor.stdout ?? "", doctor.stderr ?? "");
    if (doctor.error) throw doctor.error;
    if (doctor.status !== 0) throw new Error(`Claude-Mem doctor exited with ${doctor.status}`);
    exitCode = 0;
    messages.push("suite install completed");
  } catch (error) {
    restoreConfiguration(backup);
    messages.push(`suite install failed: ${error instanceof Error ? error.message : String(error)}`);
  } finally {
    rmSync(backup.directory, { recursive: true, force: true });
    mkdirSync(dirname(logPath), { recursive: true });
    writeFileSync(logPath, `${sanitizeLog(messages.join("\n"))}\n`, "utf8");
  }
  return exitCode;
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : "";
if (invokedPath === resolve(fileURLToPath(import.meta.url))) {
  const rootIndex = process.argv.indexOf("--root");
  if (rootIndex < 0 || !process.argv[rootIndex + 1]) {
    console.error("Usage: install-suite.mjs --root <suite-resource-directory>");
    process.exit(2);
  }
  process.exit(runSuiteInstall(resolve(process.argv[rootIndex + 1])));
}
