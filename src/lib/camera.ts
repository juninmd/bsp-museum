/** Conversão da câmera do Three.js (Y para cima) para as coordenadas do mapa GoldSrc (Z para cima). */

export interface MapCamera {
  x: number;
  y: number;
  z: number;
  /** graus, 0 = +X, 90 = +Y, em [0, 360) */
  yaw: number;
}

/** `pos` e `forward` no espaço do Three.js: (x, y, z) -> mapa (x, -z, y). */
export function cameraToMap(pos: readonly [number, number, number], forward: readonly [number, number, number]): MapCamera {
  const [px, py, pz] = pos;
  const [fx, , fz] = forward;
  let yaw = (Math.atan2(-fz, fx) * 180) / Math.PI;
  if (yaw < 0) yaw += 360;
  if (Object.is(yaw, -0)) yaw = 0;
  return { x: px, y: pz === 0 ? 0 : -pz, z: py, yaw };
}

/** `x y z · yaw` com valores inteiros, para o HUD do viewer. */
export function formatCamera(c: MapCamera): { pos: string; yaw: string } {
  const n = (v: number) => String(Math.round(v));
  return { pos: `${n(c.x)} ${n(c.y)} ${n(c.z)}`, yaw: n(c.yaw) };
}
