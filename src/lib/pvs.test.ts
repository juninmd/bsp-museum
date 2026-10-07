import { describe, expect, test } from "bun:test";
import type { PvsData } from "../types.ts";
import { decompressVis, Pvs } from "./pvs.ts";

const b64 = (bytes: number[]) => btoa(String.fromCharCode(...bytes));

/**
 * Mesmo mapa de duas salas do teste em Rust (`bsp/tree.rs`): plano x=0, folha 1 na
 * frente (x>=0), folha 2 atrás; folha 0 é o "fora do mundo" sólido.
 */
function twoRooms(): PvsData {
  return {
    planes: [1, 0, 0, 0],
    nodes: [0, -2, -3],
    // contents, visofs, primeira marksurface, quantidade
    leaves: [-2, -1, 0, 0, -1, 0, 0, 2, -1, 1, 2, 2],
    marksurfaces: [0, 1, 1, 2],
    visibility: b64([0b01, 0b11]),
    headnode: 0,
    visleafs: 2,
    world_first_face: 0,
    world_face_count: 3,
  };
}

describe("Pvs", () => {
  test("classifica o ponto na folha certa", () => {
    const pvs = new Pvs(twoRooms());
    expect(pvs.leafAt(10, 0, 0)).toBe(1);
    expect(pvs.leafAt(-10, 0, 0)).toBe(2);
  });

  test("folha 1 vê só as faces dela; folha 2 vê as duas salas (união)", () => {
    const pvs = new Pvs(twoRooms());
    expect([...pvs.facesVisibleFrom(1)!]).toEqual([1, 1, 0]);
    expect([...pvs.facesVisibleFrom(2)!]).toEqual([1, 1, 1]);
  });

  test("folha 0 e folha sem vis significam 'desenhe tudo'", () => {
    const pvs = new Pvs(twoRooms());
    expect(pvs.facesVisibleFrom(0)).toBeNull();
    expect(pvs.facesVisibleFrom(-1)).toBeNull();
  });

  test("face fora do modelo 0 (brush entity) é sempre visível", () => {
    const pvs = new Pvs({ ...twoRooms(), world_first_face: 1, world_face_count: 2 });
    const visible = pvs.predicate(pvs.facesVisibleFrom(1))!;
    expect(visible(0)).toBe(true); // antes do mundo
    expect(visible(99)).toBe(true); // depois do mundo
  });

  test("árvore com ciclo ou plano inválido não trava", () => {
    expect(new Pvs({ ...twoRooms(), nodes: [0, 0, 0] }).leafAt(1, 0, 0)).toBe(-1);
    expect(new Pvs({ ...twoRooms(), nodes: [9, -2, -3] }).leafAt(1, 0, 0)).toBe(-1);
  });

  test("vis truncado devolve null em vez de esconder geometria", () => {
    const pvs = new Pvs({ ...twoRooms(), visibility: b64([0]) });
    expect(pvs.facesVisibleFrom(1)).toBeNull();
  });
});

describe("decompressVis", () => {
  test("expande o RLE de zeros", () => {
    expect([...decompressVis(Uint8Array.of(0xff, 0, 3, 0x01), 0, 5)!]).toEqual([0xff, 0, 0, 0, 1]);
  });
  test("run maior que a linha é cortada", () => {
    expect([...decompressVis(Uint8Array.of(0, 200), 0, 4)!]).toEqual([0, 0, 0, 0]);
  });
  test("truncado e zero-zero viram null", () => {
    expect(decompressVis(Uint8Array.of(0xff), 0, 4)).toBeNull();
    expect(decompressVis(Uint8Array.of(0, 0), 0, 4)).toBeNull();
  });
});
