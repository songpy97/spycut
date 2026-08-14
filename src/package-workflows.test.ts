import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const root = resolve(import.meta.dirname, "..");
const read = (path: string) => readFileSync(resolve(root, path), "utf8");

describe("release packaging policy", () => {
  it("does not package Windows through the macOS script", () => {
    const macScript = read("scripts/package-macos.sh");
    const retiredScript = read("scripts/package-all-macos.sh");

    expect(macScript).not.toMatch(/cargo-xwin|docker|orbctl|makensis|windows_installer/i);
    expect(macScript).toContain('mkdir -p "$(dirname "$mac_dmg")"');
    expect(retiredScript).toContain("has been retired");
    expect(retiredScript).toContain("exit 2");
    expect(existsSync(resolve(root, "src-tauri/tauri.cross-windows.conf.json"))).toBe(false);
    expect(existsSync(resolve(root, "scripts/prepare-ffmpeg-windows-cross.sh"))).toBe(false);
  });

  it("smoke-tests native Windows installers before upload", () => {
    const workflow = read(".github/workflows/ci.yml");
    const windowsScript = read("scripts/package-windows.ps1");

    expect(workflow).toContain("./scripts/package-windows.ps1 -SkipChecks -SmokeTest");
    expect(workflow).toContain("bundle/nsis/*.sha256");
    expect(workflow).toContain("bash scripts/package-macos.sh");
    expect(workflow).toContain("bundle/macos/*_checksums.txt");
    expect(workflow).not.toContain("docs/release/*_checksums.txt");
    expect(workflow).toContain("gh release create");
    expect(workflow).toContain("startsWith(github.ref, 'refs/tags/v')");
    expect(windowsScript).toContain("Running the NSIS self-check and silent installation");
    expect(windowsScript).toContain("Get-ExistingSpyCutInstallations");
    expect(windowsScript).toContain("Uninstall smoke test did not remove spycut.exe");
    expect(windowsScript).toContain("WaitForExit($TimeoutSeconds * 1000)");
    expect(windowsScript).toContain("Remove-Item $installer.FullName, $checksumPath");
  });

  it("packages both macOS architectures on native runners", () => {
    const workflow = read(".github/workflows/ci.yml");
    const macScript = read("scripts/package-macos.sh");

    expect(workflow).toContain("os: macos-15\n");
    expect(workflow).toContain("artifact: SpyCut-macOS-arm64");
    expect(workflow).toContain("os: macos-15-intel");
    expect(workflow).toContain("artifact: SpyCut-macOS-x64");
    expect(macScript).toContain("aarch64-apple-darwin");
    expect(macScript).toContain("x86_64-apple-darwin");
    expect(macScript).toContain('SpyCut_${app_version}_${mac_arch}_checksums.txt');
  });
});
