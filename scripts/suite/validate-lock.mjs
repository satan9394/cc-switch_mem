import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("../..", import.meta.url)));
const lock = JSON.parse(readFileSync(resolve(root, "suite-lock.json"), "utf8"));
const expected = {
  ccSwitch: "satan9394/cc-switch_mem",
  claudeMem: "satan9394/claude-mem_local",
};

if (lock.schemaVersion !== 1) throw new Error("Unsupported suite lock schema");
for (const [component, repository] of Object.entries(expected)) {
  if (lock[component]?.repository !== repository) {
    throw new Error(`${component} must use ${repository}`);
  }
  if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(lock[component]?.version ?? "")) {
    throw new Error(`${component} version must be exact semver`);
  }
}
if (lock.claudeMem.tag !== `v${lock.claudeMem.version}`) {
  throw new Error("Claude-Mem tag and version disagree");
}
const nodeUrl = new URL(lock.node.url);
if (nodeUrl.protocol !== "https:" || nodeUrl.hostname !== "nodejs.org") {
  throw new Error("Node runtime must use the pinned official HTTPS host");
}
if (!/^[a-f0-9]{64}$/.test(lock.node.sha256)) throw new Error("Invalid Node SHA-256");

console.log(
  `Suite lock valid: CC Switch ${lock.ccSwitch.version}, Claude-Mem ${lock.claudeMem.version}, Node ${lock.node.version}`,
);
