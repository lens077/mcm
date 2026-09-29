// The application's names (Sumery brand system, work/docs/brand/DESIGN.md §1).
//
// Brand and product are two different names doing two different jobs: users
// search for "Sumery Pyxis" and have to find it among everything else called
// Pyxis, while the menu bar only has room for "Pyxis". A rename touches half a
// dozen files and a missed one fails silently, so they are pinned here.
//
// The engineering names stay: repository `mcm`, crates `mcm-*`, binary
// `mcm-app`, the `.mcm` file format and the `MCM_*` environment variables.
import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path: string) => readFileSync(join(ROOT, path), "utf8");
const config = JSON.parse(read("src-tauri/tauri.conf.json"));
const infoPlist = read("src-tauri/Info.plist");

function plistValue(key: string): string | undefined {
  return infoPlist.match(new RegExp(`<key>${key}</key>\\s*<string>([^<]*)</string>`))?.[1];
}

describe("Sumery Pyxis identity", () => {
  it("the searchable name carries the brand", () => {
    expect(config.productName).toBe("Sumery Pyxis");
    expect(plistValue("CFBundleDisplayName")).toBe("Sumery Pyxis");
  });

  it("the menu bar name is the product alone and fits", () => {
    const menuName = plistValue("CFBundleName") ?? "";
    expect(menuName).toBe("Pyxis");
    // Apple truncates CFBundleName past 15 characters.
    expect(menuName.length).toBeLessThanOrEqual(15);
  });

  it("the bundle id belongs to the brand", () => {
    expect(config.identifier).toBe("com.sumery.pyxis");
  });

  it("the window title and the page title are the searchable name", () => {
    expect(config.app.windows[0].title).toBe("Sumery Pyxis");
    expect(read("index.html")).toMatch(/<title>Sumery Pyxis<\/title>/);
  });

  it("Windows shows the brand where Windows has room for it", () => {
    expect(config.bundle.publisher).toBe("Sumery");
    expect(config.bundle.copyright).toBe("© Sumery");
    expect(config.bundle.windows.nsis.startMenuFolder).toBe("Sumery");
    expect(config.bundle.windows.nsis.installMode).toBe("currentUser");
  });

  it("the binary keeps its engineering name", () => {
    // scripts/measure-startup.mjs and tests/e2e/wdio.conf.ts launch mcm-app.
    expect(config.mainBinaryName).toBeUndefined();
    expect(read("src-tauri/Cargo.toml")).toMatch(/^name = "mcm-app"$/m);
  });

  it("the sidebar carries the brand mark, not the old MCM label", () => {
    const app = read("src/app/App.tsx");
    expect(app).toContain('className="brand-kicker">SUMERY · PYXIS<');
    expect(app).toContain('className="brand-name">Pyxis<');
    expect(app).not.toMatch(/className="eyebrow">MCM</);
  });
});
