import { afterEach, beforeEach, describe, expect, test } from "bun:test";

/** DOM mínimo: só o que modal.ts usa (querySelector, hidden, focus, listeners). */
class FakeEl {
  hidden = true;
  textContent = "";
  constructor(readonly id: string) {}
  focus() {
    fake.activeElement = this;
  }
  replaceChildren() {}
  addEventListener() {}
}

const fake = {
  activeElement: null as FakeEl | null,
  els: new Map<string, FakeEl>(),
  querySelector(sel: string) {
    return this.els.get(sel) ?? null;
  },
  addEventListener() {},
  removeEventListener() {},
};

let mod: typeof import("./modal.ts");
const realDocument = (globalThis as { document?: unknown }).document;

beforeEach(async () => {
  fake.els = new Map(["#modal", "#modal-scrim", "#modal-title", "#modal-body", "#modal-close"].map((s) => [s, new FakeEl(s)]));
  (globalThis as { document?: unknown }).document = fake;
  mod = await import("./modal.ts");
  mod.closeModal();
});

afterEach(() => {
  (globalThis as { document?: unknown }).document = realDocument;
});

describe("modal focus return", () => {
  test("closing returns focus to the element that opened it", () => {
    const opener = new FakeEl("#opener");
    opener.focus();
    mod.openModal("A");
    mod.closeModal();
    expect(fake.activeElement).toBe(opener);
  });

  test("replacing an open dialog keeps the original opener as focus target", () => {
    const opener = new FakeEl("#opener");
    opener.focus();
    mod.openModal("Settings");
    // ex.: trocar idioma reabre as configurações com o foco dentro do diálogo
    new FakeEl("#set-lang").focus();
    mod.openModal("Settings (en)");
    mod.closeModal();
    expect(fake.activeElement).toBe(opener);
  });
});
