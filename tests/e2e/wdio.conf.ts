// WebDriver configuration for the Windows smoke suite (research.md R8).
// macOS has no tauri-driver support, so it runs the same checklist through
// `mcm-app --selftest` instead; both platforms gate on one scenario list.
import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import path from "node:path";
import process from "node:process";

const projectRoot = path.resolve(__dirname, "../..");
const isWindows = process.platform === "win32";
const binaryName = isWindows ? "mcm-app.exe" : "mcm-app";
const application = path.join(projectRoot, "src-tauri", "target", "release", binaryName);

let driver: ChildProcess | null = null;

export const config: WebdriverIO.Config = {
  runner: "local",
  specs: [path.join(__dirname, "smoke.spec.ts")],
  maxInstances: 1,
  // tauri-driver proxies to the platform WebView driver on 4444.
  hostname: "127.0.0.1",
  port: 4444,
  capabilities: [
    {
      "tauri:options": { application },
      browserName: "wry",
    } as WebdriverIO.Capabilities,
  ],
  reporters: ["spec"],
  framework: "mocha",
  mochaOpts: { ui: "bdd", timeout: 120_000 },
  waitforTimeout: 15_000,

  // Build the release binary once before the suite starts.
  onPrepare: () => {
    const built = spawnSync("cargo", ["build", "--release", "-p", "mcm-app"], {
      cwd: projectRoot,
      stdio: "inherit",
      shell: isWindows,
    });
    if (built.status !== 0) {
      throw new Error("cargo build --release failed; cannot run the smoke suite");
    }
  },

  beforeSession: () => {
    driver = spawn("tauri-driver", [], { stdio: [null, process.stdout, process.stderr] });
  },

  afterSession: () => {
    driver?.kill();
    driver = null;
  },
};
