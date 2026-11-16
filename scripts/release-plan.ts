// Calcula a próxima versão a partir dos commits semânticos desde a última tag vX.Y.Z.
// Uso (CI): bun scripts/release-plan.ts  → imprime JSON {release, version, tag, notes}.
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

export type Bump = "major" | "minor" | "patch" | null;
export interface Commit {
  subject: string;
  body: string;
}

const HEADER = /^(\w+)(\([^)]*\))?(!)?:\s*(.+)$/;

export function bumpOf(c: Commit): Bump {
  const m = HEADER.exec(c.subject.trim());
  if (!m) return null;
  if (m[3] || /^BREAKING[ -]CHANGE:/m.test(c.body)) return "major";
  if (m[1] === "feat") return "minor";
  if (m[1] === "fix" || m[1] === "perf") return "patch";
  return null;
}

const RANK = { patch: 1, minor: 2, major: 3 } as const;

export function nextVersion(base: string, commits: Commit[]): { version: string; bump: Bump } {
  let top: Bump = null;
  for (const c of commits) {
    const b = bumpOf(c);
    if (b && (!top || RANK[b] > RANK[top])) top = b;
  }
  if (!top) return { version: base, bump: null };
  const [maj = 0, min = 0, pat = 0] = base.split(".").map(Number);
  const version = top === "major" ? `${maj + 1}.0.0` : top === "minor" ? `${maj}.${min + 1}.0` : `${maj}.${min}.${pat + 1}`;
  return { version, bump: top };
}

const TITLES: Record<string, string> = { feat: "Novidades", fix: "Correções", perf: "Desempenho" };

export function notesOf(commits: Commit[]): string {
  const groups = new Map<string, string[]>();
  for (const c of commits) {
    const m = HEADER.exec(c.subject.trim());
    if (!m) continue;
    const title = m[3] ? "Mudanças incompatíveis" : TITLES[m[1] ?? ""];
    if (!title) continue;
    const scope = m[2] ? `**${m[2].slice(1, -1)}:** ` : "";
    groups.set(title, [...(groups.get(title) ?? []), `- ${scope}${m[4]}`]);
  }
  return [...groups].map(([t, l]) => `### ${t}\n${l.join("\n")}`).join("\n\n");
}

const semver = (t: string) => /^v(\d+)\.(\d+)\.(\d+)$/.test(t);
const cmp = (a: string, b: string) => {
  const n = (v: string) => v.slice(1).split(".").map(Number);
  const [x, y] = [n(a), n(b)];
  return (x[0] ?? 0) - (y[0] ?? 0) || (x[1] ?? 0) - (y[1] ?? 0) || (x[2] ?? 0) - (y[2] ?? 0);
};

function git(...args: string[]): string {
  return execFileSync("git", args, { encoding: "utf8" });
}

if (import.meta.main) {
  const last = git("tag", "--list", "v*").split("\n").filter(semver).sort(cmp).pop();
  const base = last ? last.slice(1) : (JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8")).version as string);
  const raw = git("log", last ? `${last}..HEAD` : "HEAD", "--format=%s%x1f%b%x1e");
  const commits = raw
    .split("\x1e")
    .map((r) => r.trim())
    .filter(Boolean)
    .map((r) => {
      const [subject = "", body = ""] = r.split("\x1f");
      return { subject, body };
    });
  const { version, bump } = nextVersion(base, commits);
  console.log(JSON.stringify({ release: bump !== null, version, tag: `v${version}`, notes: notesOf(commits) }));
}
