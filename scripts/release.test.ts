import { expect, test } from "bun:test";
import {
  bootstrapRange,
  bumpVersion,
  createReleaseManifest,
  hasRequiredAssets,
  planReleases,
  releaseBump,
  releaseNotes,
  verifyReleaseManifest,
} from "./release";

const a = "a".repeat(40);
const b = "b".repeat(40);
const c = "c".repeat(40);
const d = "d".repeat(40);

function assets(version: string): string[] {
  const bundles = [
    `bsp-museum_${version}_x64-setup.exe`,
    `bsp-museum_${version}_x64_en-US.msi`,
    `bsp-museum_${version}_aarch64.app.tar.gz`,
    `bsp-museum_${version}_x64.app.tar.gz`,
    `bsp-museum_${version}_amd64.AppImage`,
    `bsp-museum_${version}_amd64.deb`,
    `bsp-museum-${version}-1.x86_64.rpm`,
  ];
  return ["latest.json", ...bundles.flatMap((name) => [name, `${name}.sig`])];
}

test("uses Conventional Commits for patch, minor and major releases", () => {
  expect(planReleases([
    { sha: a, message: "docs: update README" },
    { sha: b, message: "fix(sync): avoid duplicate upload" },
    { sha: c, message: "feat: add compact mode" },
    { sha: d, message: "feat(api)!: replace the vault format" },
  ], new Map(), new Map(), "v0.3.1")).toEqual([
    { sha: b, tag: "v0.3.2", version: "0.3.2", previousTag: "v0.3.1" },
    { sha: c, tag: "v0.4.0", version: "0.4.0", previousTag: "v0.3.2" },
    { sha: d, tag: "v1.0.0", version: "1.0.0", previousTag: "v0.4.0" },
  ]);
  expect(bumpVersion("4.9.8", "patch")).toBe("4.9.9");
});

test("reads the conventional title and breaking footer from merge commits", () => {
  expect(releaseBump("Merge pull request #31 from feature\n\nfeat(settings): compact mode")).toBe("minor");
  expect(releaseBump("Merge pull request #32 from refactor\n\nrefactor: vault\n\nBREAKING CHANGE: new envelope")).toBe("major");
  expect(releaseBump("chore: refresh dependencies")).toBeUndefined();
  expect(releaseBump("perf: speed up startup")).toBe("patch");
  expect(releaseBump("refactor: split modules")).toBe("patch");
  expect(releaseBump("refactor: split\n\n* feat: add filter")).toBe("minor");
});

test("retries drafts and skips published releases with complete installers", () => {
  const tags = new Map([["v0.3.2", a], ["v0.3.3", b]]);
  expect(planReleases([
    { sha: a, message: "fix: first" },
    { sha: b, message: "fix: second" },
  ], tags, new Map([
    ["v0.3.2", { draft: false, assets: assets("0.3.2") }],
    ["v0.3.3", { draft: true, assets: [] }],
  ]), "v0.3.1")).toEqual([
    { sha: b, tag: "v0.3.3", version: "0.3.3", previousTag: "v0.3.2" },
  ]);
});

test("published releases missing a signed installer are rejected", () => {
  expect(hasRequiredAssets(assets("0.3.2"), "0.3.2")).toBe(true);
  expect(() => planReleases([{ sha: a, message: "fix: release" }], new Map([["v0.3.2", a]]), new Map([
    ["v0.3.2", { draft: false, assets: assets("0.3.2").filter((name) => !name.endsWith(".AppImage.sig")) }],
  ]), "v0.3.1")).toThrow("instaladores completos");
});

// Without the updater signing key there is no latest.json and no .sig: the release is still valid with the installers.
test("a release built without the signing key only needs the plain installers", () => {
  const plain = assets("0.3.2").filter((name) => !name.endsWith(".sig") && name !== "latest.json");
  expect(hasRequiredAssets(plain, "0.3.2")).toBe(true);
  expect(hasRequiredAssets(plain.filter((name) => !name.endsWith(".rpm")), "0.3.2")).toBe(false);
});

test("the first release of a repository folds the whole history into the tip", () => {
  expect(bootstrapRange([
    { sha: a, message: "feat: first", taggable: true },
    { sha: b, message: "fix: second", taggable: true },
    { sha: c, message: "ci: third", taggable: true },
  ]).map((commit) => commit.taggable)).toEqual([false, false, true]);
  expect(planReleases(bootstrapRange([
    { sha: a, message: "feat: first", taggable: true },
    { sha: b, message: "fix: second", taggable: true },
  ]), new Map(), new Map(), "v0.5.0")).toEqual([
    { sha: b, tag: "v0.6.0", version: "0.6.0", previousTag: "v0.5.0" },
  ]);
});

test("builds and verifies one updater manifest after parallel uploads", () => {
  const names = assets("0.3.2");
  const release = {
    tag_name: "v0.3.2",
    draft: true,
    body: "## Alterações\n",
    assets: names.map((name, index) => ({ id: index + 1, name })),
  };
  const signatures = new Map(names.filter((name) => name.endsWith(".sig")).map((name) => [name, `signature:${name}`]));
  const manifest = createReleaseManifest(
    release,
    signatures,
    "v0.3.2",
    "0.3.2",
    release.body,
    "juninmd/bsp-museum",
    "2026-09-22T00:00:00.000Z",
  );
  expect(Object.keys(manifest.platforms)).toHaveLength(11);
  expect(() => verifyReleaseManifest(manifest, release, "v0.3.2", "0.3.2", "juninmd/bsp-museum")).not.toThrow();
  manifest.platforms["windows-x86_64"]!.url = "https://api.github.com/repos/juninmd/bsp-museum/releases/assets/999";
  expect(() => verifyReleaseManifest(manifest, release, "v0.3.2", "0.3.2", "juninmd/bsp-museum")).toThrow("windows-x86_64");
});

test("manifest generation rejects a missing signature", () => {
  const names = assets("0.3.2");
  const release = {
    tag_name: "v0.3.2",
    draft: true,
    body: "notas",
    assets: names.map((name, index) => ({ id: index + 1, name })),
  };
  expect(() => createReleaseManifest(release, new Map(), "v0.3.2", "0.3.2", "notas", "juninmd/bsp-museum"))
    .toThrow("assinatura ausente");
});

test("refuses to reuse a semantic version whose tag points to another commit", () => {
  expect(() => planReleases([{ sha: a, message: "fix: release" }], new Map([["v0.3.2", b]]), new Map(), "v0.3.1"))
    .toThrow("outro commit");
});

const repo = "juninmd/bsp-museum";
const candidate = { sha: a, tag: "v0.4.0", version: "0.4.0", previousTag: "v0.3.1" };

test("a merged pull request title becomes a release note under the right heading, without the merge line", () => {
  const notes = releaseNotes("Merge pull request #24 from feature\n\nfeat: aba Status [API]", candidate, repo);
  expect(notes.startsWith("✨ 1 novidade\n")).toBe(true);
  expect(notes).toContain("## ✨ Novidades");
  expect(notes).toContain("Aba Status \\[API\\] ([#24](https://github.com/juninmd/bsp-museum/pull/24))");
  expect(notes).toContain("/compare/v0.3.1...v0.4.0");
  expect(notes).not.toContain("Merge pull request");
});

test("the notes group the commits of a merged pull request by type and leave out docs, tests and chores", () => {
  const details = [
    { sha: b, subject: "feat(alert): snooze for 1, 5 or 10 minutes" },
    { sha: c, subject: "fix(agenda): keep ringing while locked" },
    { sha: d, subject: "docs: update README" },
    { sha: a, subject: "test: cover the snooze" },
    { sha: b, subject: "feat(alert): snooze for 1, 5 or 10 minutes" },
    { sha: c, subject: "perf: batch the vault writes" },
  ];
  const notes = releaseNotes("Merge pull request #7 from x\n\nfeat: alerts that wait", candidate, repo, details);
  expect(notes.split("\n")[0]).toBe("✨ 1 novidade · 🐛 1 correção · ⚡ 1 ganho de desempenho");
  expect(notes).toContain("> Alerts that wait ([#7](https://github.com/juninmd/bsp-museum/pull/7))");
  expect(notes.indexOf("## ✨ Novidades")).toBeLessThan(notes.indexOf("## 🐛 Correções"));
  expect(notes.indexOf("## 🐛 Correções")).toBeLessThan(notes.indexOf("## ⚡ Desempenho"));
  expect(notes).toContain(`- **alert** · Snooze for 1, 5 or 10 minutes ([\`${b.slice(0, 7)}\`](https://github.com/juninmd/bsp-museum/commit/${b}))`);
  expect(notes).not.toContain("README");
  expect(notes).not.toContain("cover the snooze");
  expect(notes.match(/Snooze for 1, 5 or 10 minutes/g)).toHaveLength(1);
});

test("a squash commit lists its parts from the bullets in the body", () => {
  const message = "feat: alerts that wait (#9)\n\n* feat(alert): snooze menu\n\nbody text\n\n* fix: ring twice\n\nCo-authored-by: A <a@x>";
  const notes = releaseNotes(message, candidate, repo);
  expect(notes).toContain("✨ 1 novidade · 🐛 1 correção");
  expect(notes).toContain("**alert** · Snooze menu ([#9](https://github.com/juninmd/bsp-museum/pull/9))");
  expect(notes).toContain("Ring twice");
});

test("a breaking change gets its own warning heading first", () => {
  const notes = releaseNotes("feat(api)!: replace the vault format", candidate, repo);
  expect(notes.split("\n")[0]).toBe("⚠️ 1 mudança incompatível");
  expect(notes.indexOf("## ⚠️ Mudanças incompatíveis")).toBeLessThan(notes.indexOf("## ⬇️ Baixar"));
});

test("a title that is not a Conventional Commit still produces a note, with Markdown escaped", () => {
  const notes = releaseNotes("Atualiza *tudo* <b>agora</b>", candidate, repo);
  expect(notes).toContain("Atualiza \\*tudo\\* \\<b\\>agora\\</b\\>");
});

test("the download table points at files that the release workflow really publishes", () => {
  const version = "0.4.0";
  const published = new Set(assets(version));
  const notes = releaseNotes("fix: x", candidate, repo);
  const links = [...notes.matchAll(/releases\/download\/v0\.4\.0\/([^)]+)\)/g)].map((m) => m[1] ?? "");
  expect(links).toHaveLength(7);
  for (const name of links.filter((n) => !n.endsWith(".dmg"))) expect(published.has(name), name).toBe(true);
  expect(links.filter((n) => n.endsWith(".dmg"))).toEqual([`bsp-museum_${version}_aarch64.dmg`, `bsp-museum_${version}_x64.dmg`]);
});

test("the notes are the same every time, because the updater manifest must match the release body", () => {
  const message = "feat: alerts (#3)\n\n* fix: one\n* feat: two";
  expect(releaseNotes(message, candidate, repo)).toBe(releaseNotes(message, candidate, repo));
});
