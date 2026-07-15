import { describe, expect, test } from "vitest";
// @ts-expect-error The shipped installer helper is intentionally plain Node ESM.
import {
  buildInstallArgs,
  sanitizeLog,
} from "../../scripts/suite/install-suite.mjs";

describe("suite installer orchestration", () => {
  test("builds a secret-free child invocation", () => {
    expect(buildInstallArgs("C:\\suite\\claude-mem")).toEqual([
      "C:\\suite\\claude-mem\\dist\\npx-cli\\index.js",
      "install",
      "--ide",
      "claude-code",
      "--provider",
      "cc-switch",
      "--runtime",
      "worker",
      "--auto-start",
    ]);
  });

  test("redacts token-like values from the suite log", () => {
    const output = sanitizeLog(
      "ANTHROPIC_API_KEY=secret-value\nAuthorization: Bearer abc123\napi_key: topsecret",
    );
    expect(output).not.toContain("secret-value");
    expect(output).not.toContain("abc123");
    expect(output).not.toContain("topsecret");
    expect(output).toContain("[REDACTED]");
  });
});
