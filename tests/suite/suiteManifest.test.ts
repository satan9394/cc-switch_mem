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
});
