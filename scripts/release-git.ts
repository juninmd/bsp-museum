import { execFileSync } from "node:child_process";
import type { Commit } from "./release";
import type { Detail } from "./release-notes";

export function run(command: string, args: string[], cwd?: string): string {
  return execFileSync(command, args, { encoding: "utf8", cwd, stdio: ["ignore", "pipe", "pipe"] }).trim();
}

/** GITHUB_TOKEN has no `workflows` scope: it cannot make a ref at a commit whose .github/workflows differs from every branch tip. */
export function canTag(commitWorkflows: string, tipWorkflows: string): boolean {
  return commitWorkflows === "" || commitWorkflows === tipWorkflows;
}

export function workflowsTree(rev: string, cwd?: string): string {
  return run("git", ["ls-tree", rev, ".github/workflows"], cwd).split(/\s+/)[2] ?? "";
}

export function readCommits(shas: string[], tipRev: string, cwd?: string): Commit[] {
  const tipWorkflows = workflowsTree(tipRev, cwd);
  return shas.map((sha) => ({
    sha,
    message: run("git", ["log", "-1", "--format=%B", sha], cwd),
    taggable: canTag(workflowsTree(sha, cwd), tipWorkflows),
  }));
}

/** A merged pull request brings its own commits: they make a richer list than the merge commit's title. */
export function mergedCommits(sha: string, cwd?: string): Detail[] {
  const parents = run("git", ["rev-list", "--parents", "-n", "1", sha], cwd).split(" ").slice(1);
  if (parents.length !== 2) return [];
  const rows = run("git", ["log", "--reverse", "--format=%H%x09%s", "--max-count=60", `${parents[0]}..${parents[1]}`], cwd);
  return (rows ? rows.split("\n") : []).map((row) => {
    const [commit, ...subject] = row.split("\t");
    return { sha: commit ?? "", subject: subject.join("\t") };
  });
}

/** A release that folded earlier commits lists the whole range in its notes, not only the cut. Empty when nothing was folded. */
export function foldedDetails(previousTag: string, sha: string, cwd?: string): Detail[] {
  const rows = run("git", ["rev-list", "--first-parent", "--reverse", `${previousTag}..${sha}`], cwd);
  const range = rows ? rows.split("\n") : [];
  if (range.length < 2) return [];
  return range.flatMap((commit) => {
    const merged = mergedCommits(commit, cwd);
    if (merged.length > 0) return merged;
    const [title = "", ...body] = run("git", ["log", "-1", "--format=%B", commit], cwd).split(/\r?\n/);
    const bullets = body.flatMap((line) => /^[*-]\s+(.+)$/.exec(line.trim())?.[1] ?? []);
    return (bullets.length > 0 ? bullets : [title]).map((subject) => ({ sha: commit, subject }));
  });
}
