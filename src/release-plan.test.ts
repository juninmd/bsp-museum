import { describe, expect, test } from "bun:test";
import { bumpOf, nextVersion, notesOf } from "../scripts/release-plan.ts";

const c = (subject: string, body = "") => ({ subject, body });

describe("plano de release", () => {
  test("feat → minor, fix → patch, o maior vence", () => {
    expect(nextVersion("0.7.0", [c("fix: a"), c("feat(ui): b")])).toEqual({ version: "0.8.0", bump: "minor" });
    expect(nextVersion("0.7.0", [c("fix: a")])).toEqual({ version: "0.7.1", bump: "patch" });
  });
  test("breaking por ! ou rodapé → major", () => {
    expect(bumpOf(c("feat!: x"))).toBe("major");
    expect(bumpOf(c("fix: x", "BREAKING CHANGE: y"))).toBe("major");
    expect(nextVersion("0.7.0", [c("feat!: x")]).version).toBe("1.0.0");
  });
  test("chore/docs/ci e mensagens livres não geram release", () => {
    expect(nextVersion("0.7.0", [c("chore: a"), c("docs: b"), c("Merge pull request #1")])).toEqual({ version: "0.7.0", bump: null });
  });
  test("notas agrupam por tipo", () => {
    const n = notesOf([c("feat(ui): tela"), c("fix: bug"), c("chore: x")]);
    expect(n).toContain("### Novidades\n- **ui:** tela");
    expect(n).toContain("### Correções\n- bug");
    expect(n).not.toContain("chore");
  });
});
