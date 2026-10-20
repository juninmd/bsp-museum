import { describe, expect, test } from "bun:test";
import * as THREE from "three";
import { MdlRig, emptyTextures, toWorld, type Model } from "./mdl-rig.ts";

/** Dois triângulos no plano do jogo (z = 0): o 0 usa a textura 0 e o 1 a textura 1. */
function model(): Model {
  const positions = [
    // tri 0 em x = -20..-10
    -20, 0, 0, -10, 0, 0, -15, 10, 0,
    // tri 1 em x = 10..20
    10, 0, 0, 20, 0, 0, 15, 10, 0,
  ];
  return {
    positions,
    uvs: [0, 0, 1, 0, 0.5, 1, 0, 0, 1, 0, 0.5, 1],
    texindex: [0, 1],
    textures: [
      { name: "a", png: null },
      { name: "b", png: null },
    ],
    sequences: [],
    sequence_info: [],
    local_positions: positions,
    vert_bones: [0, 0, 0, 0, 0, 0],
    num_bones: 1,
    skin_families: [
      [0, 1],
      [1, 0],
    ],
  };
}

describe("MdlRig", () => {
  test("as skins são jogos de materiais sobre a mesma geometria", () => {
    const rig = new MdlRig(model());
    const tex = emptyTextures(2);
    const a = rig.addLook([0, 1], tex);
    const b = rig.addLook([1, 0], tex);
    const geos = (l: typeof a) => l.group.children.map((c) => (c as THREE.Mesh).geometry);
    expect(geos(a)).toHaveLength(2);
    expect(geos(a)).toEqual(geos(b)); // mesmos buffers, nenhuma geometria nova
    const mats = (l: typeof a) => l.group.children.map((c) => (c as THREE.Mesh).material);
    expect(mats(a)[0]).not.toBe(mats(b)[0]);
    rig.dispose();
  });

  test("descartar um look solta só os materiais; a geometria só cai com o rig", () => {
    const rig = new MdlRig(model());
    const look = rig.addLook([], emptyTextures(2));
    const mesh = look.group.children[0] as THREE.Mesh;
    let geoDisposed = 0;
    let matDisposed = 0;
    mesh.geometry.addEventListener("dispose", () => geoDisposed++);
    (mesh.material as THREE.Material).addEventListener("dispose", () => matDisposed++);
    look.dispose();
    expect(matDisposed).toBe(1);
    expect(geoDisposed).toBe(0);
    expect(rig.root.children).not.toContain(look.group);
    look.dispose(); // idempotente
    expect(matDisposed).toBe(1);
    rig.dispose();
    expect(geoDisposed).toBe(1);
  });

  test("pick devolve o triângulo de origem sob o raio e respeita a camada", () => {
    const rig = new MdlRig(model());
    const look = rig.addLook([0, 1], emptyTextures(2), 1);
    const camera = new THREE.PerspectiveCamera(50, 1, 0.1, 1000);
    const [cx, cy, cz] = toWorld(15, 3, 0);
    camera.position.set(cx, cy + 60, cz);
    camera.lookAt(cx, cy, cz);
    camera.updateMatrixWorld();
    camera.updateProjectionMatrix();
    expect(rig.pick(0, 0, camera, 1)).toBe(1);
    expect(rig.pick(0, 0, camera, 0)).toBe(-1); // o look está na camada 1
    look.group.visible = false;
    expect(rig.pick(0, 0, camera, 1)).toBe(-1);
    rig.dispose();
  });

  test("a pose nova move a geometria e o pick segue a malha", () => {
    const m = model();
    const rig = new MdlRig(m);
    const look = rig.addLook([0, 1], emptyTextures(2));
    const moved = m.positions.map((v, i) => (i % 3 === 0 && i >= 9 ? v - 25 : v)); // tri 1 vai de x = 10..20 pra -15..-5
    rig.setVertexPositions(moved);
    const camera = new THREE.PerspectiveCamera(50, 1, 0.1, 1000);
    const [cx, cy, cz] = toWorld(-10, 3, 0);
    camera.position.set(cx, cy + 60, cz);
    camera.lookAt(cx, cy, cz);
    camera.updateMatrixWorld();
    camera.updateProjectionMatrix();
    expect(rig.pick(0, 0, camera, 0)).toBe(1);
    look.dispose();
    rig.dispose();
  });
});
