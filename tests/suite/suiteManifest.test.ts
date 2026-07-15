import { readFileSync } from "node:fs";
import { describe, expect, test } from "vitest";

const readJson = (path: string) => JSON.parse(readFileSync(path, "utf8"));

describe("MEM suite manifest", () => {
  test("pins only maintained forks and exact runtime hashes", () => {
    const lock = readJson("suite-lock.json");
    expect(lock.ccSwitch).toMatchObject({
      version: "3.17.0-mem.1",
      repository: "satan9394/cc-switch_mem",
    });
    expect(lock.claudeMem).toMatchObject({
      version: "13.11.0-local.4",
      repository: "satan9394/claude-mem_local",
    });
    expect(lock.node.sha256).toMatch(/^[a-f0-9]{64}$/);
    expect(lock.node.url).toBe(
      "https://nodejs.org/dist/v24.18.0/node-v24.18.0-win-x64.zip",
    );
  });

  test("keeps release versions, fork updater, workflow, and security notes aligned", () => {
    const pkg = readJson("package.json");
    const tauri = readJson("src-tauri/tauri.conf.json");
    const cargo = readFileSync("src-tauri/Cargo.toml", "utf8");
    const workflow = readFileSync(".github/workflows/release.yml", "utf8");
    const notes = readFileSync(
      "docs/release-notes/v3.17.0-mem.1-zh.md",
      "utf8",
    );

    expect(pkg.version).toBe("3.17.0-mem.1");
    expect(tauri.version).toBe(pkg.version);
    expect(cargo).toContain('version = "3.17.0-mem.1"');
    expect(tauri.plugins.updater.endpoints).toEqual([
      "https://github.com/satan9394/cc-switch_mem/releases/latest/download/latest.json",
    ]);
    expect(workflow).toContain(
      "CC-Switch-MEM-Suite-v3.17.0-mem.1-Windows-x64-Setup.exe",
    );
    expect(workflow).toContain("SHA256SUMS.txt");
    expect(workflow).toContain("$Name.sig");
    for (const phrase of [
      "Claude-Mem Local",
      "本地",
      "模型供应商",
      "SmartScreen",
    ]) {
      expect(notes).toContain(phrase);
    }
  });
});
