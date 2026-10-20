import { describe, expect, test } from "bun:test";
import { HULL_MAX_TRIANGLES, HULL_MAX_WINDOWS, HULL_WINDOW, HullPager, hullPositionsToWorld, planHullWindows, type HullWindowInfo } from "./hulls.ts";

/** resposta de uma janela como o backend a devolveria */
function window(offset: number, limit: number, total: number, truncated = false): HullWindowInfo {
  const triangles = Math.max(0, Math.min(limit, total - offset));
  const end = offset + triangles;
  return { offset, triangles, total_triangles: total, next_offset: end < total ? end : null, truncated };
}

/** roda o pager contra um "backend" falso e devolve as janelas pedidas */
function drain(pager: HullPager, total: number): { offset: number; limit: number }[] {
  const asked: { offset: number; limit: number }[] = [];
  for (let req = pager.next(); req; req = pager.next()) {
    asked.push(req);
    pager.accept(window(req.offset, req.limit, total));
  }
  return asked;
}

describe("planHullWindows", () => {
  test("cobre o hull em janelas contíguas", () => {
    const plan = planHullWindows(250_000, 100_000);
    expect(plan).toEqual([
      { offset: 0, limit: 100_000 },
      { offset: 100_000, limit: 100_000 },
      { offset: 200_000, limit: 50_000 },
    ]);
  });
  test("respeita os tetos de triângulos e de janelas", () => {
    expect(planHullWindows(5_000_000).reduce((n, w) => n + w.limit, 0)).toBe(HULL_MAX_TRIANGLES);
    expect(planHullWindows(10_000_000, 1000, 10_000_000, 5)).toHaveLength(5);
    expect(planHullWindows(0)).toEqual([]);
  });
});

describe("HullPager", () => {
  test("segue next_offset até acabar", () => {
    const pager = new HullPager(100, 10_000, 20);
    const asked = drain(pager, 250);
    expect(asked.map((w) => w.offset)).toEqual([0, 100, 200]);
    const p = pager.progress();
    expect(p.loaded).toBe(250);
    expect(p.total).toBe(250);
    expect(p.windows).toBe(3);
    expect(p.done).toBe(true);
    expect(p.capped).toBe(false);
    expect(p.fraction).toBe(1);
  });

  test("hull pequeno cabe numa janela só", () => {
    const pager = new HullPager();
    const asked = drain(pager, 12);
    expect(asked).toEqual([{ offset: 0, limit: HULL_WINDOW }]);
  });

  test("hull vazio termina sem loop", () => {
    const pager = new HullPager();
    expect(drain(pager, 0)).toHaveLength(1);
    expect(pager.progress().done).toBe(true);
  });

  test("teto de triângulos corta a carga e avisa", () => {
    const pager = new HullPager(100, 250, 20);
    const asked = drain(pager, 10_000);
    expect(asked.map((w) => w.limit)).toEqual([100, 100, 50]); // a última janela é aparada pelo teto
    const p = pager.progress();
    expect(p.loaded).toBe(250);
    expect(p.capped).toBe(true);
    expect(p.done).toBe(true);
    expect(p.fraction).toBe(1); // a barra chega ao fim do que será desenhado; `capped` avisa do corte
  });

  test("teto de janelas protege de backend que nunca termina", () => {
    const pager = new HullPager(10, 1_000_000, HULL_MAX_WINDOWS);
    let n = 0;
    for (let req = pager.next(); req; req = pager.next()) {
      pager.accept({ offset: req.offset, triangles: 10, total_triangles: 1_000_000, next_offset: req.offset + 10, truncated: false });
      n++;
    }
    expect(n).toBe(HULL_MAX_WINDOWS);
    expect(pager.progress().capped).toBe(true);
  });

  test("next_offset que não avança encerra em vez de repetir a janela", () => {
    const pager = new HullPager(100, 10_000, 20);
    const req = pager.next()!;
    pager.accept({ offset: req.offset, triangles: 100, total_triangles: 500, next_offset: 0, truncated: false });
    expect(pager.next()).toBeNull();
  });

  test("janela vazia no meio também encerra", () => {
    const pager = new HullPager(100, 10_000, 20);
    pager.accept({ offset: 0, triangles: 0, total_triangles: 500, next_offset: 100, truncated: false });
    expect(pager.next()).toBeNull();
  });

  test("cancelar interrompe e truncated do backend é lembrado", () => {
    const pager = new HullPager(100, 10_000, 20);
    pager.accept(window(0, 100, 300, true));
    pager.cancel();
    expect(pager.next()).toBeNull();
    expect(pager.progress().truncated).toBe(true);
  });

  test("progresso parcial", () => {
    const pager = new HullPager(100, 10_000, 20);
    pager.accept(window(0, 100, 400));
    const p = pager.progress();
    expect(p.fraction).toBeCloseTo(0.25, 6);
    expect(p.planned).toBe(4);
    expect(p.done).toBe(false);
  });
});

describe("hullPositionsToWorld", () => {
  test("Z-up vira Y-up (x, z, -y)", () => {
    expect([...hullPositionsToWorld([1, 2, 3, -4, 5, 6])]).toEqual([1, 3, -2, -4, 6, -5]);
  });
  test("ignora vértice incompleto no fim", () => {
    expect(hullPositionsToWorld([1, 2, 3, 4]).length).toBe(3);
  });
});
