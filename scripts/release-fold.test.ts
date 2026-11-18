import { expect, test } from "bun:test";
import { planReleases } from "./release";

const [a = "", b = "", c = "", d = ""] = ["a", "b", "c", "d"].map((letter) => letter.repeat(40));

// GITHUB_TOKEN has no `workflows` scope: it gets a 403 on the draft and the tag of a commit whose
// .github/workflows no branch tip shares. A release it cannot cut is folded into the next one it can.
test("folds commits the token cannot tag into the next one it can, with the strongest bump", () => {
  expect(planReleases([
    { sha: a, message: "feat: agenda day picker", taggable: false },
    { sha: b, message: "ci: restore the clippy gate", taggable: false },
    { sha: c, message: "fix(deps): update dependencies", taggable: false },
    { sha: d, message: "ci: waive one lint", taggable: true },
  ], new Map(), new Map(), "v0.24.0")).toEqual([
    { sha: d, tag: "v0.25.0", version: "0.25.0", previousTag: "v0.24.0" },
  ]);
});

test("a folded breaking change makes the cut a major release", () => {
  expect(planReleases([
    { sha: a, message: "feat!: new vault format", taggable: false },
    { sha: b, message: "fix: typo", taggable: true },
  ], new Map(), new Map(), "v0.24.0")).toEqual([
    { sha: b, tag: "v1.0.0", version: "1.0.0", previousTag: "v0.24.0" },
  ]);
});

test("releases a taggable commit on its own and folds only what follows", () => {
  expect(planReleases([
    { sha: a, message: "fix: first", taggable: true },
    { sha: b, message: "feat: second", taggable: false },
    { sha: c, message: "docs: third", taggable: true },
  ], new Map(), new Map(), "v0.3.1")).toEqual([
    { sha: a, tag: "v0.3.2", version: "0.3.2", previousTag: "v0.3.1" },
    { sha: c, tag: "v0.4.0", version: "0.4.0", previousTag: "v0.3.2" },
  ]);
});

test("a release already cut does not release again on the next taggable commit", () => {
  expect(planReleases([
    { sha: a, message: "fix: first", taggable: true },
    { sha: b, message: "docs: second", taggable: true },
  ], new Map(), new Map(), "v0.3.1")).toEqual([
    { sha: a, tag: "v0.3.2", version: "0.3.2", previousTag: "v0.3.1" },
  ]);
});

test("a range without a releasable commit stays unreleased even when untaggable", () => {
  expect(planReleases([
    { sha: a, message: "ci: bump an action", taggable: false },
    { sha: b, message: "docs: readme", taggable: true },
  ], new Map(), new Map(), "v0.3.1")).toEqual([]);
});

test("a draft left for a folded commit is retargeted to the new cut", () => {
  expect(planReleases([
    { sha: a, message: "feat: drafted earlier", taggable: false },
    { sha: b, message: "ci: bump an action", taggable: true },
  ], new Map(), new Map([["v0.4.0", { draft: true, assets: [] }]]), "v0.3.1")).toEqual([
    { sha: b, tag: "v0.4.0", version: "0.4.0", previousTag: "v0.3.1" },
  ]);
});
