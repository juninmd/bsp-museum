import { describe, expect, test } from "bun:test";
import {
  cyclePhase,
  fillStyleTable,
  frameIndex,
  moverPose,
  patternValues,
  renderFxFactor,
  routePosition,
  scrollOffset,
  styleValueAt,
  type MoverSpec,
} from "./anim.ts";

describe("lightstyles", () => {
  test("letra m vale 1, a vale 0, z passa de 2", () => {
    expect(patternValues("m")).toEqual([1]);
    expect(patternValues("a")).toEqual([0]);
    expect(patternValues("z")[0]!).toBeCloseTo((25 * 22) / 264, 5);
    expect(patternValues("?")).toEqual([1]); // fora de a..z = luz normal
  });

  test("valor pelo tempo: uma letra a cada 0,1 s, em ciclo", () => {
    const def = { id: 4, pattern: "ac" };
    expect(styleValueAt(def, 0)).toBe(0);
    expect(styleValueAt(def, 0.099)).toBe(0);
    expect(styleValueAt(def, 0.1)).toBeCloseTo((2 * 22) / 264, 5);
    expect(styleValueAt(def, 0.2)).toBe(0); // voltou ao início
    expect(styleValueAt(def, 10.05)).toBe(0);
  });

  test("usa `values` e `fps` do backend quando vêm", () => {
    const def = { id: 1, values: [0.5, 1.5, 2], fps: 2 };
    expect(styleValueAt(def, 0)).toBe(0.5);
    expect(styleValueAt(def, 0.5)).toBe(1.5);
    expect(styleValueAt(def, 1.2)).toBe(2);
    expect(styleValueAt(def, 1.5)).toBe(0.5);
  });

  test("estilo 0 / sem padrão / tempo inválido = luz normal", () => {
    expect(styleValueAt({ id: 0, pattern: "m" }, 123.4)).toBe(1);
    expect(styleValueAt({ id: 9 }, 3)).toBe(1);
    expect(styleValueAt({ id: 9, pattern: "a" }, Number.NaN)).toBe(1);
    expect(styleValueAt({ id: 9, pattern: "a" }, -5)).toBe(0); // tempo negativo trava no primeiro quadro
  });

  test("tabela do shader: estática sem animação, ligável apagada vale 0", () => {
    const table = new Float32Array(64);
    const defs = [
      { id: 0, pattern: "m" },
      { id: 2, pattern: "ab" },
      { id: 33, pattern: "m", switchable: true },
    ];
    fillStyleTable(table, defs, 0.15, true, new Set([33]));
    expect(table[0]).toBe(1);
    expect(table[2]).toBeCloseTo((1 * 22) / 264, 5);
    expect(table[33]).toBe(0);
    expect(table[40]).toBe(1); // estilo sem definição
    fillStyleTable(table, defs, 0.15, false);
    expect(table[2]).toBe(1);
    expect(table[33]).toBe(1);
    fillStyleTable(table, defs, 0.15, false, new Set([33]));
    expect(table[33]).toBe(0); // apagada mesmo sem animação
  });
});

describe("texturas animadas", () => {
  test("10 quadros por segundo, em ciclo", () => {
    expect(frameIndex(4, 0)).toBe(0);
    expect(frameIndex(4, 0.099)).toBe(0);
    expect(frameIndex(4, 0.1)).toBe(1);
    expect(frameIndex(4, 0.35)).toBe(3);
    expect(frameIndex(4, 0.4)).toBe(0);
    expect(frameIndex(10, 1.05)).toBe(0);
  });
  test("um quadro só, tempo ruim ou intervalo inválido ficam no zero", () => {
    expect(frameIndex(1, 5)).toBe(0);
    expect(frameIndex(0, 5)).toBe(0);
    expect(frameIndex(3, Number.NaN)).toBe(0);
    expect(frameIndex(3, 1, 0)).toBe(0);
  });
  test("intervalo próprio do grupo", () => {
    expect(frameIndex(3, 0.5, 0.25)).toBe(2);
  });
});

describe("rolagem de textura", () => {
  test("positivo anda para +s: o offset de UV diminui (fração do ciclo)", () => {
    // 64 texels/s numa textura de 64 de largura = um ciclo por segundo
    expect(scrollOffset(64, 0.25, 64)).toBeCloseTo(0.75, 6); // -0,25 -> 0,75
    expect(scrollOffset(64, 1, 64)).toBeCloseTo(0, 6);
  });
  test("negativo anda para -s", () => {
    expect(scrollOffset(-32, 1, 64)).toBeCloseTo(0.5, 6);
    expect(scrollOffset(-64, 0.25, 64)).toBeCloseTo(0.25, 6);
  });
  test("escala pela largura: textura maior rola mais devagar", () => {
    const small = scrollOffset(-16, 1, 64);
    const big = scrollOffset(-16, 1, 128);
    expect(small).toBeCloseTo(0.25, 6);
    expect(big).toBeCloseTo(0.125, 6);
  });
  test("fica em [0, 1) e não perde precisão com sessão longa", () => {
    const v = scrollOffset(37.5, 86_400, 128);
    expect(v).toBeGreaterThanOrEqual(0);
    expect(v).toBeLessThan(1);
  });
  test("entrada inválida não quebra", () => {
    expect(scrollOffset(10, 1, 0)).toBe(0);
    expect(scrollOffset(Number.NaN, 1, 64)).toBe(0);
  });
});

describe("renderfx", () => {
  test("sem efeito = 1; pulso fica em [0, 1]; strobe só tem dois níveis", () => {
    expect(renderFxFactor(0, 3)).toBe(1);
    for (const fx of [1, 2, 3, 4, 12, 13]) {
      for (let i = 0; i < 100; i++) {
        const v = renderFxFactor(fx, i * 0.037);
        expect(v).toBeGreaterThanOrEqual(0);
        expect(v).toBeLessThanOrEqual(1);
      }
    }
    const levels = new Set<number>();
    for (let i = 0; i < 200; i++) levels.add(renderFxFactor(9, i * 0.05));
    expect([...levels].sort()).toEqual([0.08, 1]);
  });
});

describe("movers", () => {
  const door: MoverSpec = { kind: "door", speed: 100, wait: 1, open_offset: [0, 0, 100], rot_delta: null, start_open: false };

  test("fase do vai-e-vem: fecha, abre, espera, volta", () => {
    expect(cyclePhase(1, 1, 0)).toBe(0);
    expect(cyclePhase(1, 1, 0.5)).toBeCloseTo(0.5, 6);
    expect(cyclePhase(1, 1, 1.5)).toBe(1); // esperando aberta
    expect(cyclePhase(1, 1, 2.5)).toBeCloseTo(0.5, 6); // voltando
    expect(cyclePhase(1, 1, 3.5)).toBe(0); // esperando fechada
    expect(cyclePhase(1, 1, 4.5)).toBeCloseTo(0.5, 6); // ciclo seguinte
  });

  test("porta desliza do fechado ao aberto e volta", () => {
    expect(moverPose(door, 0).offset).toEqual([0, 0, 0]);
    // 100 u a 100 u/s = 1 s de viagem; depois espera
    expect(moverPose(door, 1.2).offset[2]).toBeCloseTo(100, 4);
    expect(moverPose(door, 100).rot).toEqual([0, 0, 0]);
  });

  test("mapa começa aberto: a porta fecha andando no sentido contrário", () => {
    const open = moverPose({ ...door, start_open: true }, 1.2);
    expect(open.offset[2]).toBeCloseTo(-100, 4);
  });

  test("porta rotativa gira rot_delta; ventilador gira sem parar", () => {
    const rdoor: MoverSpec = { kind: "door_rotating", speed: 90, wait: 1, open_offset: null, rot_delta: [0, 90, 0], start_open: false };
    expect(moverPose(rdoor, 1.1).rot[1]).toBeCloseTo(90, 4);
    const fan: MoverSpec = { kind: "rotating", speed: 100, wait: 0, open_offset: null, rot_delta: [0, 0, 120], start_open: false };
    expect(moverPose(fan, 0.5).rot[2]).toBeCloseTo(60, 6);
    expect(moverPose(fan, 3).rot[2]).toBeCloseTo(360, 6);
  });

  test("viagem curta demais é esticada para a animação ser legível", () => {
    const tiny = moverPose({ ...door, open_offset: [0, 0, 1] }, 0.2);
    expect(tiny.offset[2]).toBeGreaterThan(0); // não pula direto
    expect(tiny.offset[2]).toBeLessThan(1);
  });

  test("mover sem deslocamento nem rotação fica parado", () => {
    const still: MoverSpec = { ...door, open_offset: null, rot_delta: null };
    expect(moverPose(still, 3)).toEqual({ offset: [0, 0, 0], rot: [0, 0, 0] });
  });
});

describe("rota de trem", () => {
  const route = {
    points: [
      [0, 0, 0],
      [100, 0, 0],
      [100, 100, 0],
    ] as [number, number, number][],
    speeds: [0, 0, 0],
    waits: [0, 0, 0],
    looped: true,
  };

  test("anda ponto a ponto na velocidade do trem e fecha o circuito", () => {
    expect(routePosition(route, 100, 0)).toEqual([0, 0, 0]);
    const half = routePosition(route, 100, 0.5);
    expect(half[0]).toBeCloseTo(50, 4);
    const corner = routePosition(route, 100, 1.0);
    expect(corner[0]).toBeCloseTo(100, 4);
    expect(routePosition(route, 100, 1.5)[1]).toBeCloseTo(50, 4);
    // volta ao início: 100 + 100 + hypot(100,100) unidades
    const total = 100 + 100 + Math.hypot(100, 100);
    const end = routePosition(route, 100, total / 100 - 1e-9);
    expect(end[0]).toBeCloseTo(0, 3);
  });

  test("speed do corner muda a velocidade do trecho que chega nele", () => {
    const fast = { ...route, speeds: [0, 200, 0] };
    expect(routePosition(fast, 100, 0.25)[0]).toBeCloseTo(50, 4);
  });

  test("rota aberta vai e volta; ponto único e rota vazia não quebram", () => {
    const open = { ...route, looped: false };
    // ida 200 u + volta 200 u = 4 s; aos 3,5 s está voltando entre os pontos 1 e 0
    const back = routePosition(open, 100, 3.5);
    expect(back[0]).toBeCloseTo(50, 4);
    expect(routePosition({ ...route, points: [[5, 6, 7]] }, 100, 9)).toEqual([5, 6, 7]);
    expect(routePosition({ ...route, points: [] }, 100, 9)).toEqual([0, 0, 0]);
  });

  test("espera no corner segura o trem no ponto", () => {
    const waiting = { ...route, waits: [0, 2, 0] };
    expect(routePosition(waiting, 100, 2)).toEqual([100, 0, 0]);
  });
});
