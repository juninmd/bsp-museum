import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";

function workflowText(workflow: string): string {
  return readFileSync(new URL(`../.github/workflows/${workflow}`, import.meta.url), "utf8");
}

function clippyCommands(workflow: string): string[] {
  return workflowText(workflow).split(/\r?\n/).filter((line) => line.includes("cargo clippy"));
}

// A red CI is fixed in the code. A bot once dropped -D warnings to get green, and the release queue then
// stalled on a commit whose lint could not be fixed anymore.
test.each(["ci.yml", "release-commit.yml"])("%s fails on any clippy warning", (workflow) => {
  const commands = clippyCommands(workflow);
  expect(commands.length).toBeGreaterThan(0);
  for (const command of commands) expect(command).toContain("-D warnings");
});

test.each(["ci.yml", "release-commit.yml"])("%s waives no lint", (workflow) => {
  for (const command of clippyCommands(workflow)) expect(command).not.toContain(" -A ");
});

// The version comes from the commits: a release must be cut on a push to master, and the schedule has to be able
// to resume a draft the runner left behind.
test("the release is triggered by commits on master and by a schedule", () => {
  const text = workflowText("release.yml");
  expect(text).toMatch(/push:\s*\r?\n\s*branches: \[master\]/);
  expect(text).toContain("schedule:");
  expect(text).toContain("workflow_dispatch:");
});

// A resumed draft keeps the commit it was first made for. Plan may have folded that commit into a later cut,
// so the draft is pointed at the commit being built, or its publish would tag a commit the token cannot tag.
test("a resumed draft is retargeted to the commit being released", () => {
  const edit = workflowText("release-commit.yml").split(/\r?\n/).find((line) => line.includes("gh release edit") && line.includes("--notes-file"));
  expect(edit).toContain('--target "$RELEASE_SHA"');
});

// prepare and the installers run beside verify to shorten a release; this gate is what still keeps a red commit unpublished.
test("a release is published only after verify passed", () => {
  const text = workflowText("release-commit.yml");
  const job = text.split(/\r?\n  publicar:/)[1]?.split(/\r?\n  [a-z]/)[0] ?? "";
  const needs = /needs:\s*\[([^\]]*)\]/.exec(job)?.[1] ?? "";
  expect(needs.split(",").map((name) => name.trim())).toContain("verify");
});

// Two near-identical debug caches of 1.3 GB each pushed the release caches out of the repo's 10 GiB; keep one.
test("the CI rust job and the release verify share one debug cache key and env", () => {
  for (const workflow of ["ci.yml", "release-commit.yml"]) {
    const text = workflowText(workflow);
    expect(text).toMatch(/shared-key: debug\r?\n/);
    expect(text).toMatch(/CARGO_PROFILE_DEV_DEBUG: line-tables-only\r?\n/);
  }
});
