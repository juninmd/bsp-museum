import { describe, expect, test } from "bun:test";
import { cameraToMap, formatCamera } from "./camera.ts";

describe("câmera Three.js -> mapa GoldSrc", () => {
  test("posição: (x, y, z) vira (x, -z, y)", () => {
    expect(cameraToMap([10, 50, -20], [1, 0, 0])).toEqual({ x: 10, y: 20, z: 50, yaw: 0 });
  });

  test("yaw: +X é 0, +Y do mapa (−Z do Three) é 90, −X é 180", () => {
    expect(cameraToMap([0, 0, 0], [1, 0, 0]).yaw).toBe(0);
    expect(cameraToMap([0, 0, 0], [0, 0, -1]).yaw).toBeCloseTo(90, 6);
    expect(cameraToMap([0, 0, 0], [-1, 0, 0]).yaw).toBeCloseTo(180, 6);
    expect(cameraToMap([0, 0, 0], [0, 0, 1]).yaw).toBeCloseTo(270, 6);
  });

  test("inclinação não muda o yaw", () => {
    expect(cameraToMap([0, 0, 0], [0.5, -0.8, 0]).yaw).toBe(0);
  });

  test("formatação arredonda", () => {
    expect(formatCamera({ x: 1.4, y: -2.6, z: 100.5, yaw: 44.6 })).toEqual({ pos: "1 -3 101", yaw: "45" });
  });
});
