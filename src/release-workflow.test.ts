import {
  chmodSync, copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { execFileSync, spawnSync } from "node:child_process";
import { describe, expect, it } from "vitest";

const root = resolve(import.meta.dirname, "..");
const scriptPath = resolve(root, "scripts/release.sh");
const bashPath =
  process.platform === "win32"
    ? execFileSync("where.exe", ["bash"], { encoding: "utf8" }).split(/\r?\n/).find(Boolean)!
    : execFileSync("bash", ["-lc", "command -v bash"], { encoding: "utf8" }).trim();

function runScript(...args: string[]) {
  return spawnSync(bashPath, [scriptPath, ...args], {
    cwd: root,
    encoding: "utf8",
    timeout: 5_000
  });
}

function git(cwd: string, ...args: string[]): string {
  return execFileSync("git", args, { cwd, encoding: "utf8" }).trim();
}

function commandPath(command: string): string {
  return execFileSync("bash", ["-lc", `command -v ${command}`], { encoding: "utf8" }).trim();
}

function shellQuote(value: string): string {
  return `'${value.replaceAll("'", `'\\''`)}'`;
}

function createReleaseFixture(packageRunner: "pnpm" | "npm" = "pnpm") {
  const base = mkdtempSync(join(tmpdir(), "spycut-release-test-"));
  const repo = join(base, "repo");
  const remote = join(base, "remote.git");
  mkdirSync(repo, { recursive: true });
  git(base, "init", "--bare", remote);
  git(repo, "init", "-b", "main");
  git(repo, "config", "user.name", "SpyCut Test");
  git(repo, "config", "user.email", "spycut-test@example.invalid");

  mkdirSync(join(repo, "scripts"), { recursive: true });
  mkdirSync(join(repo, "src-tauri/src"), { recursive: true });
  mkdirSync(join(repo, ".github/workflows"), { recursive: true });
  copyFileSync(scriptPath, join(repo, "scripts/release.sh"));
  chmodSync(join(repo, "scripts/release.sh"), 0o755);
  writeFileSync(join(repo, "package.json"), '{\n  "name": "spycut",\n  "version": "1.0.0"\n}\n');
  writeFileSync(join(repo, "src-tauri/tauri.conf.json"), '{\n  "productName": "SpyCut",\n  "version": "1.0.0"\n}\n');
  writeFileSync(join(repo, "src-tauri/Cargo.toml"), '[package]\nname = "spycut"\nversion = "1.0.0"\nedition = "2024"\n');
  writeFileSync(join(repo, "src-tauri/Cargo.lock"), 'version = 4\n\n[[package]]\nname = "spycut"\nversion = "1.0.0"\n');
  writeFileSync(join(repo, "src-tauri/src/lib.rs"), "");
  writeFileSync(join(repo, ".gitignore"), "src-tauri/target/\n");
  writeFileSync(
    join(repo, ".github/workflows/ci.yml"),
    'on:\n  push:\n    tags:\n      - "v*"\nsteps:\n  - run: gh release create "$GITHUB_REF_NAME" --prerelease\n'
  );

  git(repo, "add", "-A");
  git(repo, "commit", "-m", "initial");
  git(repo, "remote", "add", "origin", remote);
  git(repo, "push", "-u", "origin", "main");
  writeFileSync(join(repo, "CHANGELOG.md"), "release fixture\n");

  const bin = join(base, "bin");
  mkdirSync(bin);
  for (const command of ["git", "node", "cargo"]) {
    writeFileSync(join(bin, command), `#!/bin/bash\nexec ${shellQuote(commandPath(command))} "$@"\n`);
    chmodSync(join(bin, command), 0o755);
  }
  if (packageRunner === "pnpm") {
    writeFileSync(join(bin, "pnpm"), "#!/bin/bash\nexit 0\n");
    chmodSync(join(bin, "pnpm"), 0o755);
  } else {
    writeFileSync(
      join(bin, "npm"),
      '#!/bin/bash\n[[ "$*" == "exec --yes --package=pnpm@11.16.0 -- pnpm check" ]] || exit 99\n'
    );
    chmodSync(join(bin, "npm"), 0o755);
  }
  return { base, repo, remote, bin };
}

function runFixtureRelease(repo: string, bin: string, input: string) {
  return spawnSync(bashPath, [join(repo, "scripts/release.sh")], {
    cwd: repo,
    encoding: "utf8",
    input,
    timeout: 30_000,
    env: { ...process.env, PATH: `${bin}:/usr/bin:/bin` }
  });
}

describe("interactive release workflow", () => {
  it.each([
    ["patch", "1.2.4"],
    ["minor", "1.3.0"],
    ["major", "2.0.0"]
  ])("calculates the next %s version without side effects", (increment, expected) => {
    const result = runScript("--calculate", "1.2.3", increment);

    expect(result.status).toBe(0);
    expect(result.stdout.trim()).toBe(expected);
    expect(result.stderr).toBe("");
  });

  it("rejects malformed versions and unknown increment types", () => {
    expect(runScript("--calculate", "1.2", "patch").status).not.toBe(0);
    expect(runScript("--calculate", "01.2.3", "patch").status).not.toBe(0);
    expect(runScript("--calculate", "1.2.3", "banana").status).not.toBe(0);
  });

  it("keeps release publication guarded and atomic", () => {
    const script = readFileSync(scriptPath, "utf8");

    expect(script).toContain("pnpm check");
    expect(script).toContain("npm exec --yes --package=pnpm@11.16.0 -- pnpm");
    expect(script).toContain('"${pnpm_command[@]}" check');
    expect(script).toContain("SPYCUT_RELEASE_IN_PROGRESS=1");
    expect(script).toContain("GIT_PAGER=cat");
    expect(script).toContain("git diff --check");
    expect(script).toContain("git tag -a");
    expect(script).toContain("git push --atomic");
    expect(script).toContain("发布已触发：$release_tag");
    expect(script).toContain("GIT_TERMINAL_PROMPT=0");
    expect(script).toContain("git add -A");
    expect(script).toContain("mktemp -d");
    expect(script).toContain("package.json");
    expect(script).toContain("src-tauri/tauri.conf.json");
    expect(script).toContain("src-tauri/Cargo.toml");
    expect(script).toContain("src-tauri/Cargo.lock");
    expect(script.match(/confirm_release/g)?.length ?? 0).toBeGreaterThanOrEqual(2);
    expect(script).not.toMatch(/push[^\n]*--force/);
    expect(script).not.toMatch(/\$[A-Za-z_][A-Za-z0-9_]*[^\x00-\x7F]/);
  });

  it("has valid Bash syntax and documents the interactive entry point", () => {
    expect(spawnSync(bashPath, ["-n", scriptPath], { cwd: root }).status).toBe(0);
    expect(runScript("--help").stdout).toContain("major");
    expect(runScript("--help").stdout).toContain("minor");
    expect(runScript("--help").stdout).toContain("patch");
  });

  it("restores version files when publication is cancelled", () => {
    const fixture = createReleaseFixture("npm");
    try {
      const result = runFixtureRelease(fixture.repo, fixture.bin, "3\ny\nn\n");

      expect(result.status, `${result.stdout}\n${result.stderr}`).toBe(0);
      expect(result.stdout).toContain("npm");
      expect(JSON.parse(readFileSync(join(fixture.repo, "package.json"), "utf8")).version).toBe("1.0.0");
      expect(readFileSync(join(fixture.repo, "src-tauri/Cargo.toml"), "utf8")).toContain('version = "1.0.0"');
      expect(git(fixture.repo, "tag", "--list")).toBe("");
      expect(git(fixture.repo, "status", "--porcelain")).toBe("?? CHANGELOG.md");
    } finally {
      rmSync(fixture.base, { recursive: true, force: true });
    }
  }, 30_000);

  it("commits pending fixes and continues an unpublished tagged release", () => {
    const fixture = createReleaseFixture();
    try {
      for (const path of [
        "package.json",
        "src-tauri/tauri.conf.json",
        "src-tauri/Cargo.toml",
        "src-tauri/Cargo.lock"
      ]) {
        const absolutePath = join(fixture.repo, path);
        writeFileSync(absolutePath, readFileSync(absolutePath, "utf8").replaceAll("1.0.0", "1.0.1"));
      }
      git(
        fixture.repo,
        "add",
        "package.json",
        "src-tauri/tauri.conf.json",
        "src-tauri/Cargo.toml",
        "src-tauri/Cargo.lock"
      );
      git(fixture.repo, "commit", "-m", "release: v1.0.1");
      git(fixture.repo, "tag", "-a", "v1.0.1", "-m", "SpyCut v1.0.1");

      const result = runFixtureRelease(fixture.repo, fixture.bin, "y\n");

      expect(result.status, `${result.stdout}\n${result.stderr}`).toBe(0);
      expect(git(fixture.repo, "log", "-1", "--format=%s")).toBe("fix: complete v1.0.1 release");
      expect(git(fixture.repo, "status", "--porcelain")).toBe("");
      expect(git(fixture.repo, "rev-parse", "HEAD")).toBe(git(fixture.repo, "rev-parse", "v1.0.1^{commit}"));
      expect(git(fixture.repo, "rev-parse", "HEAD")).toBe(git(fixture.remote, "rev-parse", "refs/heads/main"));
      expect(git(fixture.repo, "rev-parse", "HEAD")).toBe(
        git(fixture.remote, "rev-parse", "refs/tags/v1.0.1^{commit}")
      );
    } finally {
      rmSync(fixture.base, { recursive: true, force: true });
    }
  }, 30_000);

  it.skipIf(process.env.SPYCUT_RELEASE_IN_PROGRESS === "1")(
    "commits, tags and atomically pushes a confirmed patch release",
    () => {
      const fixture = createReleaseFixture();
      try {
        const result = runFixtureRelease(fixture.repo, fixture.bin, "3\ny\ny\n");

        expect(result.status, `${result.stdout}\n${result.stderr}`).toBe(0);
        expect(JSON.parse(readFileSync(join(fixture.repo, "package.json"), "utf8")).version).toBe("1.0.1");
        expect(git(fixture.repo, "log", "-1", "--format=%s")).toBe("release: v1.0.1");
        expect(git(fixture.repo, "status", "--porcelain")).toBe("");
        expect(git(fixture.repo, "rev-parse", "HEAD")).toBe(git(fixture.repo, "rev-parse", "v1.0.1^{commit}"));
        expect(git(fixture.repo, "rev-parse", "HEAD")).toBe(git(fixture.remote, "rev-parse", "refs/tags/v1.0.1^{commit}"));
      } finally {
        rmSync(fixture.base, { recursive: true, force: true });
      }
    },
    30_000
  );
});
