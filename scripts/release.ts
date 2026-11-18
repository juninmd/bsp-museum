import { appendFileSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import {
  createReleaseManifest,
  hasRequiredAssets,
  type Manifest,
  type Release,
  verifyReleaseManifest,
} from "./release-manifest";

export { createReleaseManifest, hasRequiredAssets, verifyReleaseManifest } from "./release-manifest";
export { releaseNotes, type Detail } from "./release-notes";
import { foldedDetails, mergedCommits, readCommits, run } from "./release-git";
import { releaseNotes } from "./release-notes";

export type Candidate = { sha: string; tag: string; version: string; previousTag: string };
export type Commit = { sha: string; message: string; taggable?: boolean };
export type ReleaseState = { draft: boolean; assets: string[] };
export type ReleaseBump = "major" | "minor" | "patch";

/** The branch the releases are cut from. */
const BRANCH = "origin/master";

export function commitTitle(message: string): string {
  const lines = message.split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
  return lines[0]?.startsWith("Merge pull request") ? lines[1] ?? lines[0] ?? "" : lines[0] ?? "";
}

function lineBump(line: string): ReleaseBump | undefined {
  if (/^[a-z][\w-]*(?:\([^\r\n()]+\))?!:/i.test(line)) return "major";
  const type = /^([a-z][\w-]*)(?:\([^\r\n()]+\))?:/i.exec(line)?.[1]?.toLowerCase();
  if (type === "feat") return "minor";
  if (type === "fix" || type === "perf" || type === "refactor") return "patch";
  return undefined;
}

export function releaseBump(message: string): ReleaseBump | undefined {
  const title = commitTitle(message);
  if (/^BREAKING[ -]CHANGE\s*:/im.test(message)) return "major";
  let bump = lineBump(title);
  for (const line of message.split(/\r?\n/).slice(1)) {
    const bullet = /^[*-]\s+(.+)$/.exec(line.trim())?.[1];
    if (bullet) bump = strongestBump(bump, lineBump(bullet));
  }
  return bump;
}

export function bumpVersion(version: string, bump: ReleaseBump): string {
  const match = /^(\d+)\.(\d+)\.(\d+)$/.exec(version);
  if (!match) throw new Error(`Versão inválida: ${version}`);
  const [, majorText, minorText, patchText] = match;
  const [major, minor, patch] = [Number(majorText), Number(minorText), Number(patchText)];
  if (bump === "major") return `${major + 1}.0.0`;
  if (bump === "minor") return `${major}.${minor + 1}.0`;
  return `${major}.${minor}.${patch + 1}`;
}

const BUMP_RANK: Record<ReleaseBump, number> = { patch: 1, minor: 2, major: 3 };

/** A folded range releases with the strongest bump any of its commits asked for. */
function strongestBump(left: ReleaseBump | undefined, right: ReleaseBump | undefined): ReleaseBump | undefined {
  if (!left || !right) return left ?? right;
  return BUMP_RANK[right] > BUMP_RANK[left] ? right : left;
}

export function planReleases(
  commits: Commit[],
  tags: ReadonlyMap<string, string>,
  releases: ReadonlyMap<string, ReleaseState>,
  previousTag: string,
): Candidate[] {
  let version = previousTag.replace(/^v/, "");
  let priorTag = previousTag;
  const candidates: Candidate[] = [];
  let pending: ReleaseBump | undefined;
  for (const commit of commits) {
    if (!/^[0-9a-f]{40}$/.test(commit.sha)) throw new Error(`Commit inválido: ${commit.sha}`);
    pending = strongestBump(pending, releaseBump(commit.message));
    if (!pending || commit.taggable === false) continue;
    version = bumpVersion(version, pending);
    pending = undefined;
    const tag = `v${version}`;
    const taggedSha = tags.get(tag);
    if (taggedSha && taggedSha !== commit.sha) throw new Error(`${tag} já aponta para outro commit`);
    const release = releases.get(tag);
    // A draft gets its tag only when published, so just a published release can be missing one.
    if (release && !release.draft && !taggedSha) throw new Error(`${tag} tem release sem tag local`);
    if (release && !release.draft) {
      if (!hasRequiredAssets(release.assets, version)) throw new Error(`${tag} foi publicada sem os instaladores completos`);
    } else {
      candidates.push({ sha: commit.sha, tag, version, previousTag: priorTag });
    }
    priorTag = tag;
  }
  return candidates;
}

/**
 * Before the first published release there is no previous cut to measure from: the whole history since the last tag
 * becomes one release at the branch tip, so a repository migrating to this pipeline does not publish one release per
 * old commit. The strongest bump of the range wins, and the notes list the whole range.
 */
export function bootstrapRange(commits: Commit[]): Commit[] {
  return commits.map((commit, index) => ({ ...commit, taggable: index === commits.length - 1 && commit.taggable !== false }));
}

function compareVersions(left: string, right: string): number {
  const leftParts = left.split(".").map(Number);
  const rightParts = right.split(".").map(Number);
  for (let index = 0; index < 3; index += 1) {
    const difference = (leftParts[index] ?? 0) - (rightParts[index] ?? 0);
    if (difference) return difference;
  }
  return 0;
}

function isAncestor(rev: string, branch = BRANCH): boolean {
  try {
    run("git", ["merge-base", "--is-ancestor", rev, branch]);
    return true;
  } catch {
    return false;
  }
}

function plan(): void {
  const repository = process.env.GITHUB_REPOSITORY ?? run("gh", ["repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"]);
  const tagNames = run("git", ["tag", "--list", "v[0-9]*.[0-9]*.[0-9]*"]);
  const tags = new Map((tagNames ? tagNames.split("\n") : []).map((tag) => [tag, run("git", ["rev-parse", `${tag}^{commit}`])]));
  const releaseRows = run("gh", ["api", "--paginate", `repos/${repository}/releases?per_page=100`, "--jq", ".[] | {tag: .tag_name, draft: .draft, assets: [.assets[].name]} | @json"]);
  const releases = new Map((releaseRows ? releaseRows.split("\n") : []).map((line) => {
    const row = JSON.parse(line) as { tag: string; draft: boolean; assets: string[] };
    return [row.tag, { draft: row.draft, assets: row.assets }] as const;
  }));
  const publishedTags = [...releases.entries()]
    .filter(([tag, release]) => /^v\d+\.\d+\.\d+$/.test(tag) && !release.draft && tags.has(tag) && isAncestor(tag))
    .sort(([left], [right]) => compareVersions(right.slice(1), left.slice(1)));
  let previousTag = publishedTags[0]?.[0];
  const bootstrap = !previousTag;
  if (!previousTag) {
    const reachable = [...tags.keys()].filter((tag) => isAncestor(tag)).sort((left, right) => compareVersions(right.slice(1), left.slice(1)));
    previousTag = reachable[0];
    if (!previousTag) throw new Error(`Nenhuma tag SemVer publicada foi encontrada em ${BRANCH}`);
    console.error(`Sem release publicada: o histórico desde ${previousTag} sai em um único release.`);
  } else {
    const previousRelease = releases.get(previousTag)!;
    if (!hasRequiredAssets(previousRelease.assets, previousTag.slice(1))) throw new Error(`${previousTag} foi publicada sem os instaladores completos`);
  }
  const history = run("git", ["rev-list", "--first-parent", "--reverse", `${previousTag}..${BRANCH}`]);
  const commits = readCommits(history ? history.split("\n") : [], BRANCH);
  const pending = planReleases(bootstrap ? bootstrapRange(commits) : commits, tags, releases, previousTag);
  if (process.env.GITHUB_OUTPUT) {
    appendFileSync(process.env.GITHUB_OUTPUT, `candidate=${JSON.stringify(pending[0] ?? {})}\n`);
    appendFileSync(process.env.GITHUB_OUTPUT, `more=${pending.length > 1}\n`);
  }
  console.log(JSON.stringify(pending));
}

export function notesFor(tag: string, sha: string, previousTag: string, repository: string, cwd?: string): string {
  const folded = foldedDetails(previousTag, sha, cwd);
  const candidate = { sha, tag, version: tag.slice(1), previousTag, folded: folded.length > 0 };
  const details = folded.length > 0 ? folded : mergedCommits(sha, cwd);
  return releaseNotes(run("git", ["log", "-1", "--format=%B", sha], cwd), candidate, repository, details);
}

function notes(): void {
  const [tag, sha, previousTag, path] = process.argv.slice(3);
  if (!tag || !sha || !previousTag || !path || !/^v\d+\.\d+\.\d+$/.test(tag)) throw new Error("Argumentos inválidos");
  const repository = process.env.GITHUB_REPOSITORY ?? run("gh", ["repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"]);
  writeFileSync(path, notesFor(tag, sha, previousTag, repository));
}

function manifest(): void {
  const [releasePath, signaturesPath, notesPath, outputPath, tag, version] = process.argv.slice(3);
  if (!releasePath || !signaturesPath || !notesPath || !outputPath || !tag || !version) throw new Error("Argumentos inválidos");
  const release = JSON.parse(readFileSync(releasePath, "utf8")) as Release;
  const signatures = new Map(readdirSync(signaturesPath).filter((name) => name.endsWith(".sig"))
    .map((name) => [name, readFileSync(join(signaturesPath, name), "utf8")]));
  const repository = process.env.GITHUB_REPOSITORY ?? run("gh", ["repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"]);
  const result = createReleaseManifest(release, signatures, tag, version, readFileSync(notesPath, "utf8"), repository);
  writeFileSync(outputPath, `${JSON.stringify(result, null, 2)}\n`);
}

function verify(): void {
  const [manifestPath, releasePath, tag, version] = process.argv.slice(3);
  if (!manifestPath || !releasePath || !tag || !version) throw new Error("Argumentos inválidos");
  const manifestValue = JSON.parse(readFileSync(manifestPath, "utf8")) as Manifest;
  const release = JSON.parse(readFileSync(releasePath, "utf8")) as Release;
  const repository = process.env.GITHUB_REPOSITORY ?? run("gh", ["repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"]);
  verifyReleaseManifest(manifestValue, release, tag, version, repository);
  console.log(`Updater ${tag} validado`);
}

if (import.meta.main) {
  if (process.argv[2] === "plan") plan();
  else if (process.argv[2] === "notes") notes();
  else if (process.argv[2] === "manifest") manifest();
  else if (process.argv[2] === "verify") verify();
  else throw new Error("Use plan, notes, manifest ou verify");
}
