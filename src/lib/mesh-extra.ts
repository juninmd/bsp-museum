import type { SceneEntities } from "../types/bsp-extra.ts";
import type { MeshDetail } from "../types.ts";

const EMPTY_SCENE: SceneEntities = { entities: [], routes: [], counts: {}, truncated: false };

/**
 * Garante os campos que o backend v0.7 acrescentou à malha. Backend antigo, cache de sessão
 * ou JSON montado à mão podem não trazê-los: o viewer não deve quebrar por isso.
 */
export function withExtraDefaults(mesh: MeshDetail): MeshDetail {
  mesh.face_styles ??= [];
  mesh.lightmap_layers ??= [];
  mesh.lightstyles ??= [];
  mesh.texture_anims ??= [];
  mesh.brush_models ??= [];
  mesh.scene ??= EMPTY_SCENE;
  mesh.scene.entities ??= [];
  mesh.scene.routes ??= [];
  return mesh;
}
