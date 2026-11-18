import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

const cargo = () => readFileSync(new URL("../src-tauri/Cargo.toml", import.meta.url), "utf8");

const updaterLine = () => cargo().split(/\r?\n/).find((line) => line.startsWith("tauri-plugin-updater"));

// The `zip` feature only exists for compressed updater bundles (Tauri v1 style). The upgrade is served by the
// signed bundles listed in scripts/release-manifest.ts (NSIS .exe, .app.tar.gz, AppImage), so extract_zip never
// runs. Re-enabling the feature is only safe together with a manifest that serves .zip on Windows and Linux.
test("the updater plugin ships without the zip feature the manifest never needs", () => {
  const line = updaterLine();
  expect(line).toBeDefined();
  expect(line).toContain("default-features = false");
  expect(line).not.toContain('"zip"');
  for (const needed of ["rustls-tls", "system-proxy"]) expect(line).toContain(needed);
});
