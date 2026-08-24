import type { MdlSummary, MeshDetail } from "./types.ts";
import { mount3D, type Viewer3D } from "./viewer3d.ts";

/** Bounds a partir dos vértices — `mount3D` usa isso pra enquadrar a câmera. */
function boundsOf(positions: number[]): MeshDetail["bounds"] {
  if (!positions.length) return null;
  let minX = Infinity;
  let minY = Infinity;
  let minZ = Infinity;
  let maxX = -Infinity;
  let maxY = -Infinity;
  let maxZ = -Infinity;
  for (let i = 0; i < positions.length; i += 3) {
    const x = positions[i]!;
    const y = positions[i + 1]!;
    const z = positions[i + 2]!;
    if (x < minX) minX = x;
    if (x > maxX) maxX = x;
    if (y < minY) minY = y;
    if (y > maxY) maxY = y;
    if (z < minZ) minZ = z;
    if (z > maxZ) maxZ = z;
  }
  return { mins: [minX, minY, minZ], maxs: [maxX, maxY, maxZ], size: [maxX - minX, maxY - minY, maxZ - minZ] };
}

/**
 * Visualizador de um `.mdl` isolado — reusa a mesma cena orbitável de
 * `mount3D` (luzes, controles, resize, tela cheia) montando um `MeshDetail`
 * sintético a partir do `MdlSummary`: mesma forma de posições/UV/texturas,
 * só sem spawns/céu/skybox (que não fazem sentido fora de um mapa).
 */
export function mountModelViewer(container: HTMLElement, model: MdlSummary): Viewer3D {
  const mesh: MeshDetail = {
    positions: model.positions,
    uvs: model.uvs,
    texindex: model.texindex,
    textures: model.textures,
    spawns: [],
    bounds: boundsOf(model.positions),
    skybox: null,
    wad_textures: 0,
    triangles: model.texindex.length,
    skipped: 0,
  };
  return mount3D(container, mesh, true);
}
