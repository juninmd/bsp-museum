import { describe, expect, test } from "bun:test";
import { buildTimeline, segmentAt } from "./explorer.ts";
import {
  UPRIGHT_LIMIT,
  angleVectors,
  blendFor,
  cameraBasis,
  clampPitch,
  cross,
  dot,
  gridSpec,
  length,
  opacityOf,
  orbitEye,
  quadCorners,
  rollAxes,
  segmentKey,
  spriteAxes,
  type Vec3,
  type ViewState,
} from "./sprite3d.ts";

const close = (a: Vec3, b: Vec3, d = 5) => {
  for (let i = 0; i < 3; i++) expect(a[i]!).toBeCloseTo(b[i]!, d);
};

const view = (eye: Vec3, target: Vec3 = [0, 0, 0], roll = 0): ViewState => ({ eye, basis: cameraBasis(eye, target, roll) });

describe("base da câmera", () => {
  test("olhando para +X: right = -Y (Y é a esquerda) e up = +Z", () => {
    const b = cameraBasis([-10, 0, 0], [0, 0, 0]);
    close(b.forward, [1, 0, 0]);
    close(b.right, [0, -1, 0]);
    close(b.up, [0, 0, 1]);
  });

  test("base ortonormal em qualquer órbita", () => {
    for (const [yaw, pitch] of [[0, 0], [37, 20], [-120, -45], [200, 80]] as const) {
      const eye = orbitEye([0, 0, 10], yaw, pitch, 100);
      expect(length(eye)).toBeGreaterThan(0);
      const b = cameraBasis(eye, [0, 0, 10], 25);
      expect(length(b.forward)).toBeCloseTo(1, 6);
      expect(length(b.right)).toBeCloseTo(1, 6);
      expect(length(b.up)).toBeCloseTo(1, 6);
      expect(dot(b.forward, b.right)).toBeCloseTo(0, 6);
      expect(dot(b.forward, b.up)).toBeCloseTo(0, 6);
      expect(dot(b.right, b.up)).toBeCloseTo(0, 6);
      // no motor (Y à esquerda) right × up aponta para trás do olhar
      close(cross(b.right, b.up), [-b.forward[0], -b.forward[1], -b.forward[2]]);
    }
  });

  test("olhar vertical não gera NaN", () => {
    const b = cameraBasis([0, 0, 50], [0, 0, 0]);
    for (const v of [...b.forward, ...b.right, ...b.up]) expect(Number.isFinite(v)).toBe(true);
  });

  test("rolagem de 90° troca right e up", () => {
    const r = rollAxes([1, 0, 0], [0, 0, 1], 90);
    close(r.right, [0, 0, 1]);
    close(r.up, [-1, 0, 0]);
  });

  test("órbita: yaw 0 / pitch 0 fica em +X; pitch 90 em cima", () => {
    close(orbitEye([0, 0, 0], 0, 0, 10), [10, 0, 0]);
    close(orbitEye([0, 0, 5], 90, 0, 10), [0, 10, 5]);
    close(orbitEye([0, 0, 0], 33, 90, 10), [0, 0, 10]);
  });

  test("pitch é limitado", () => {
    expect(clampPitch(120)).toBe(89.5);
    expect(clampPitch(-120)).toBe(-89.5);
    expect(clampPitch(10)).toBe(10);
  });
});

describe("AngleVectors", () => {
  test("zero = motor padrão (forward +X, right -Y, up +Z)", () => {
    const v = angleVectors([0, 0, 0]);
    close(v.forward, [1, 0, 0]);
    close(v.right, [0, -1, 0]);
    close(v.up, [0, 0, 1]);
  });
  test("yaw 90 olha para +Y; pitch positivo olha para baixo (Quake)", () => {
    close(angleVectors([0, 90, 0]).forward, [0, 1, 0]);
    close(angleVectors([90, 0, 0]).forward, [0, 0, -1]);
  });
  test("ortonormal com ângulos quaisquer", () => {
    const v = angleVectors([23, -71, 40]);
    expect(dot(v.forward, v.right)).toBeCloseTo(0, 6);
    expect(dot(v.forward, v.up)).toBeCloseTo(0, 6);
    expect(dot(v.right, v.up)).toBeCloseTo(0, 6);
    expect(length(v.up)).toBeCloseTo(1, 6);
  });
});

describe("eixos por tipo de sprite", () => {
  const origin: Vec3 = [0, 0, 0];

  test("parallel = right/up da câmera (inclusive com rolagem)", () => {
    const v = view([-100, 40, 30], origin, 30);
    const a = spriteAxes("parallel", v, origin);
    close(a.right, v.basis.right);
    close(a.up, v.basis.up);
    expect(a.valid).toBe(true);
  });

  test("parallel_upright gira só em Z, seguindo a direção do olhar", () => {
    const v = view([-100, 0, 40], origin);
    const a = spriteAxes("parallel_upright", v, origin);
    close(a.up, [0, 0, 1]);
    close(a.right, [0, -1, 0]);
    // câmera noutro lado
    const w = view([0, -100, 0], origin); // olha para +Y
    close(spriteAxes("parallel_upright", w, origin).right, [1, 0, 0]);
  });

  test("parallel_upright ignora a rolagem da câmera", () => {
    const a = spriteAxes("parallel_upright", view([-100, 0, 40], origin, 70), origin);
    close(a.right, [0, -1, 0]);
    close(a.up, [0, 0, 1]);
  });

  test("parallel_upright some quando o olhar é quase vertical", () => {
    const f = Math.sin(89.5 * (Math.PI / 180));
    expect(f).toBeGreaterThan(UPRIGHT_LIMIT);
    const top = view(orbitEye(origin, 10, 89.5, 100), origin);
    expect(spriteAxes("parallel_upright", top, origin).valid).toBe(false);
    const ok = view(orbitEye(origin, 10, 80, 100), origin);
    expect(spriteAxes("parallel_upright", ok, origin).valid).toBe(true);
  });

  test("facing_upright encara a POSIÇÃO da câmera, não a direção", () => {
    // câmera à esquerda do sprite, olhando para fora dele: parallel_upright e facing_upright divergem
    const eye: Vec3 = [-100, 0, 0];
    const lookingAway = view(eye, [-200, 50, 0]);
    close(spriteAxes("facing_upright", lookingAway, origin).right, [0, -1, 0]);
    const par = spriteAxes("parallel_upright", lookingAway, origin).right;
    expect(Math.abs(par[0]!)).toBeGreaterThan(0.1); // direção do olhar tem componente em X
    // deslocar o sprite para o lado gira o quad
    const a = spriteAxes("facing_upright", view(eye), [0, 100, 0]);
    close(a.right, normalizeXY([100, -100]));
  });

  test("facing_upright sem distância horizontal é inválido", () => {
    expect(spriteAxes("facing_upright", view([0, 0, 80]), origin).valid).toBe(false);
  });

  test("oriented é fixo no mundo (não depende da câmera) e recua 0,01", () => {
    const angles: Vec3 = [10, 45, 20];
    const a = spriteAxes("oriented", view([-100, 0, 0]), origin, angles);
    const b = spriteAxes("oriented", view([30, 80, 50]), origin, angles);
    close(a.right, b.right);
    close(a.up, b.up);
    const f = angleVectors(angles).forward;
    close(a.offset, [-0.01 * f[0], -0.01 * f[1], -0.01 * f[2]]);
  });

  test("parallel_oriented usa só o roll dos angles", () => {
    const v = view([-100, 0, 0]);
    const a = spriteAxes("parallel_oriented", v, origin, [50, 120, 90]);
    close(a.right, v.basis.up); // roll 90: right <- vup
    close(a.up, [-v.basis.right[0], -v.basis.right[1], -v.basis.right[2]]);
    close(spriteAxes("parallel_oriented", v, origin, [50, 120, 0]).right, v.basis.right);
  });
});

function normalizeXY([x, y]: [number, number]): Vec3 {
  const l = Math.hypot(x, y);
  return [x / l, y / l, 0];
}

describe("quad e origem do frame", () => {
  test("origem x/y posiciona o canto superior esquerdo (y para cima)", () => {
    const axes = spriteAxes("parallel", view([-100, 0, 0]), [0, 0, 0]); // right = -Y, up = +Z
    const [bl, br, tr, tl] = quadCorners(axes, [0, 0, 0], { origin_x: -32, origin_y: 32, width: 64, height: 64 });
    close(bl, [0, 32, -32]); // x=-32 => +Y (right = -Y)
    close(br, [0, -32, -32]);
    close(tr, [0, -32, 32]);
    close(tl, [0, 32, 32]);
  });

  test("frame deslocado e escala", () => {
    const axes = spriteAxes("parallel", view([-100, 0, 0]), [0, 0, 0]);
    const q = quadCorners(axes, [0, 0, 100], { origin_x: 0, origin_y: 0, width: 16, height: 8 }, 2);
    close(q[0], [0, 0, 100 - 16]);
    close(q[2], [0, -32, 100]);
  });

  test("soma o offset do oriented", () => {
    const axes = { right: [0, -1, 0] as Vec3, up: [0, 0, 1] as Vec3, offset: [-0.01, 0, 0] as Vec3, valid: true };
    const q = quadCorners(axes, [0, 0, 0], { origin_x: 0, origin_y: 1, width: 1, height: 1 });
    expect(q[0][0]).toBeCloseTo(-0.01, 6);
  });
});

describe("mapa de mistura", () => {
  test("normal: opaco com profundidade", () => {
    const b = blendFor("normal");
    expect(b).toMatchObject({ mode: "opaque", transparent: false, depthWrite: true, depthTest: true, additive: false });
  });
  test("additive: soma, sem profundidade", () => {
    const b = blendFor("additive");
    expect(b).toMatchObject({ mode: "additive", additive: true, depthWrite: false, depthTest: false, transparent: true });
  });
  test("index_alpha: translúcido sem escrever profundidade", () => {
    expect(blendFor("index_alpha")).toMatchObject({ mode: "alpha", transparent: true, depthWrite: false, alphaTest: 0 });
  });
  test("alpha_test: corta em 0,5", () => {
    expect(blendFor("alpha_test")).toMatchObject({ mode: "alphaTest", alphaTest: 0.5, depthWrite: true, transparent: false });
  });
  test("renderamt < 1 liga a transparência no normal e no alpha_test", () => {
    expect(blendFor("normal", "auto", 0.5)).toMatchObject({ transparent: true, depthWrite: false });
    expect(blendFor("alpha_test", "auto", 0.5)).toMatchObject({ transparent: true, alphaTest: 0.5 });
  });
  test("sobrescritas: normal num aditivo vira opaco; aditivo força soma; normal não muda os outros", () => {
    expect(blendFor("additive", "normal").mode).toBe("opaque");
    expect(blendFor("normal", "additive").mode).toBe("additive");
    expect(blendFor("index_alpha", "normal").mode).toBe("alpha");
    expect(blendFor("alpha_test", "normal").mode).toBe("alphaTest");
  });
  test("renderamt vira opacidade", () => {
    expect(opacityOf(255)).toBe(1);
    expect(opacityOf(0)).toBe(0);
    expect(opacityOf(300)).toBe(1);
    expect(opacityOf(-3)).toBe(0);
    expect(opacityOf(128)).toBeCloseTo(0.502, 3);
  });
});

describe("escolha de frame", () => {
  const frames = [
    { kind: "single" as const, durations: [], images: [{}] },
    { kind: "group" as const, durations: [0.5, 0.25], images: [{}, {}] },
    { kind: "single" as const, durations: [], images: [{}] },
  ] as unknown as Parameters<typeof buildTimeline>[0];

  test("a chave segue o segmento exibido", () => {
    const tl = buildTimeline(frames, 10); // 0..0,1 | 0,1..0,6 | 0,6..0,85 | 0,85..0,95
    const key = (t: number) => segmentKey(tl[segmentAt(tl, t, true)]);
    expect(key(0.05)).toBe("0:0");
    expect(key(0.3)).toBe("1:0");
    expect(key(0.7)).toBe("1:1");
    expect(key(0.9)).toBe("2:0");
    expect(key(0.95 + 0.05)).toBe("0:0"); // dá a volta
  });

  test("a chave é a mesma enquanto o frame não muda (textura não recria)", () => {
    const tl = buildTimeline(frames, 10);
    const keys = [0.11, 0.2, 0.4, 0.59].map((t) => segmentKey(tl[segmentAt(tl, t)]));
    expect(new Set(keys).size).toBe(1);
  });

  test("sem segmento a chave é vazia", () => {
    expect(segmentKey(undefined)).toBe("");
  });
});

describe("grade", () => {
  test("cobre o sprite e limita as células", () => {
    expect(gridSpec(64)).toEqual({ size: 256, divisions: 16 });
    expect(gridSpec(1)).toEqual({ size: 128, divisions: 8 });
    expect(gridSpec(2000).divisions).toBe(64);
    expect(gridSpec(0).size).toBe(128);
  });
});
