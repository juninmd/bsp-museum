import { afterEach, expect, test } from "bun:test";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { notesFor, planReleases } from "./release";
import { canTag, foldedDetails, readCommits, workflowsTree } from "./release-git";

const dirs: string[] = [];
afterEach(() => {
  for (const dir of dirs.splice(0)) rmSync(dir, { recursive: true, force: true });
});

/** A throwaway repository; `commit` returns the new sha. A `ci` file lands under .github/workflows. */
function repo() {
  const dir = mkdtempSync(join(tmpdir(), "bsp-museum-release-"));
  dirs.push(dir);
  const git = (...args: string[]) => execFileSync("git", ["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false", ...args], { cwd: dir, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
  git("init", "-q");
  mkdirSync(join(dir, ".github", "workflows"), { recursive: true });
  const commit = (message: string, files: Record<string, string>) => {
    for (const [name, text] of Object.entries(files)) writeFileSync(join(dir, name), text);
    git("add", "-A");
    git("commit", "-q", "-m", message);
    return git("rev-parse", "HEAD");
  };
  return { dir, git, commit };
}

const ci = (text: string) => ({ ".github/workflows/ci.yml": `name: ${text}\n` });

test("workflowsTree changes only when a file under .github/workflows does", () => {
  const { dir, commit } = repo();
  const added = commit("ci: add workflow", { ...ci("one"), "README.md": "one\n" });
  const unrelated = commit("docs: readme", { "README.md": "two\n" });
  const changed = commit("ci: change workflow", ci("two"));

  expect(workflowsTree(added, dir)).toMatch(/^[0-9a-f]{40}$/);
  expect(workflowsTree(unrelated, dir)).toBe(workflowsTree(added, dir));
  expect(workflowsTree(changed, dir)).not.toBe(workflowsTree(added, dir));
  expect(() => workflowsTree("0".repeat(40), dir)).toThrow();
});

test("a commit without a workflows folder has an empty tree and can always be tagged", () => {
  const { dir, git } = repo();
  git("commit", "-q", "--allow-empty", "-m", "docs: no workflows");
  const bare = git("rev-parse", "HEAD");
  expect(workflowsTree(bare, dir)).toBe("");
  expect(canTag("", "any-tip-tree")).toBe(true);
});

test("readCommits flags every commit whose workflows differ from the tip, and only those", () => {
  const { dir, commit } = repo();
  const first = commit("feat: first", ci("one"));
  const sameAsFirst = commit("fix: same workflows", { "a.txt": "a\n" });
  const second = commit("ci: change", ci("two"));
  const tip = commit("docs: tip", { "b.txt": "b\n" });

  expect(readCommits([first, sameAsFirst, second, tip], tip, dir).map((c) => [c.sha, c.taggable])).toEqual([
    [first, false],
    [sameAsFirst, false],
    [second, true],
    [tip, true],
  ]);
  expect(readCommits([tip], tip, dir)[0]?.message).toBe("docs: tip");
});

// The backlog that stalled the real pipeline: two releasable commits changed workflows before master moved on.
test("a backlog that changed workflows becomes one release cut at the tip, with every change in the notes", () => {
  const { dir, git, commit } = repo();
  const base = commit("fix: base release", ci("zero"));
  git("tag", "v0.1.0");
  const feat = commit("feat: agenda day picker (#75)", ci("one"));
  const fix = commit("fix(deps): update dependencies (#77)", ci("two"));
  const tip = commit("ci: waive one lint (#78)", ci("three"));

  const [candidate, ...rest] = planReleases(readCommits([feat, fix, tip], tip, dir), new Map([["v0.1.0", base]]), new Map(), "v0.1.0");
  expect(rest).toEqual([]);
  expect(candidate).toEqual({ sha: tip, tag: "v0.2.0", version: "0.2.0", previousTag: "v0.1.0" });

  expect(foldedDetails("v0.1.0", tip, dir).map((d) => d.sha)).toEqual([feat, fix, tip]);
  const text = notesFor("v0.2.0", tip, "v0.1.0", "juninmd/bsp-museum", dir);
  expect(text).toContain("✨ 1 novidade · 🐛 1 correção");
  expect(text).toContain("Agenda day picker");
  expect(text).toContain("Update dependencies");
  expect(text).not.toContain("> ");
});

test("a release that folded nothing keeps the notes of its own commit", () => {
  const { dir, git, commit } = repo();
  commit("fix: base release", ci("zero"));
  git("tag", "v0.1.0");
  const only = commit("feat: single change (#80)", ci("one"));
  expect(foldedDetails("v0.1.0", only, dir)).toEqual([]);
});
