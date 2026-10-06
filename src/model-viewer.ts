import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import type { MdlSummary } from "./types.ts";

/**
 * Cena 3D própria do visualizador avulso de `.mdl` (independente de
 * `viewer3d.ts`, que é do mapa): orbitável, com a malha dividida por textura
 * e os vértices reposicionáveis a cada quadro (skinning feito por quem chama).
 */
export interface ModelViewer {
  /** reposiciona os vértices: mesmo layout de `MdlSummary.positions` (9 floats por triângulo, espaço do jogo) */
  setVertexPositions(flat: ArrayLike<number>): void;
  /** troca a textura de cada índice de `MdlSummary.textures` (família de skin): `map[base] = textura nova` */
  setTextureMap(map: number[]): void;
  dispose(): void;
}

/** vira Z-up (GoldSrc) em Y-up (Three.js): (x,y,z) -> (x, z, -y). */
function toWorld(x: number, y: number, z: number): [number, number, number] {
  return [x, z, -y];
}

interface Bucket {
  textureIndex: number;
  /** triângulo de origem de cada triângulo do bucket */
  tris: number[];
  mesh: THREE.Mesh;
  material: THREE.MeshStandardMaterial;
}

export function mountModelViewer3D(container: HTMLElement, model: MdlSummary): ModelViewer {
  const renderer = new THREE.WebGLRenderer({ antialias: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  container.appendChild(renderer.domElement);

  const scene = new THREE.Scene();
  scene.background = new THREE.Color(0x0d1117);
  scene.add(new THREE.AmbientLight(0xffffff, 0.7));
  scene.add(new THREE.HemisphereLight(0xbfd9ff, 0x33373d, 0.6));
  const sun = new THREE.DirectionalLight(0xffffff, 0.9);
  sun.position.set(1, 2, 1.4);
  scene.add(sun);
  const fill = new THREE.DirectionalLight(0x8899bb, 0.3);
  fill.position.set(-1, -0.5, -1);
  scene.add(fill);

  const textures = model.textures.map((t) => {
    if (!t.png) return null;
    const tex = new THREE.TextureLoader().load(t.png);
    tex.colorSpace = THREE.SRGBColorSpace;
    tex.wrapS = THREE.RepeatWrapping;
    tex.wrapT = THREE.RepeatWrapping;
    return tex;
  });

  // Um mesh por textura, na ordem em que aparecem; `tris` guarda de onde cada triângulo veio.
  const byTexture = new Map<number, { tris: number[]; uvs: number[] }>();
  for (let tri = 0; tri < model.texindex.length; tri++) {
    const id = model.texindex[tri]!;
    let group = byTexture.get(id);
    if (!group) byTexture.set(id, (group = { tris: [], uvs: [] }));
    group.tris.push(tri);
    for (let k = 0; k < 6; k++) group.uvs.push(model.uvs[tri * 6 + k]!);
  }
  const buckets: Bucket[] = [];
  for (const [textureIndex, group] of byTexture) {
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.BufferAttribute(new Float32Array(group.tris.length * 9), 3));
    geometry.setAttribute("uv", new THREE.Float32BufferAttribute(group.uvs, 2));
    const material = new THREE.MeshStandardMaterial({
      roughness: 0.9,
      metalness: 0,
      side: THREE.DoubleSide,
      alphaTest: 0.5, // texturas mascaradas (grades, cabelo) têm alfa 0 no "buraco"
    });
    const mesh = new THREE.Mesh(geometry, material);
    mesh.frustumCulled = false; // a pose muda a cada quadro
    scene.add(mesh);
    buckets.push({ textureIndex, tris: group.tris, mesh, material });
  }

  const setTextureMap = (map: number[]) => {
    for (const b of buckets) {
      const id = map[b.textureIndex] ?? b.textureIndex;
      const tex = textures[id] ?? null;
      b.material.map = tex;
      b.material.color.set(tex ? 0xffffff : 0x8b98a5);
      b.material.needsUpdate = true;
    }
  };
  setTextureMap([]);

  const setVertexPositions = (flat: ArrayLike<number>) => {
    for (const b of buckets) {
      const attr = b.mesh.geometry.getAttribute("position") as THREE.BufferAttribute;
      for (let t = 0; t < b.tris.length; t++) {
        const src = b.tris[t]! * 9;
        for (let v = 0; v < 3; v++) {
          const s = src + v * 3;
          const [x, y, z] = toWorld(flat[s]!, flat[s + 1]!, flat[s + 2]!);
          attr.setXYZ(t * 3 + v, x, y, z);
        }
      }
      attr.needsUpdate = true;
      b.mesh.geometry.computeVertexNormals();
    }
  };
  setVertexPositions(model.positions);

  // Enquadra a câmera na pose de repouso.
  const box = new THREE.Box3();
  for (const b of buckets) {
    b.mesh.geometry.computeBoundingBox();
    box.union(b.mesh.geometry.boundingBox!);
  }
  if (box.isEmpty()) box.set(new THREE.Vector3(-50, -50, -50), new THREE.Vector3(50, 50, 50));
  const center = box.getCenter(new THREE.Vector3());
  const maxDim = Math.max(...box.getSize(new THREE.Vector3()).toArray(), 1);

  const camera = new THREE.PerspectiveCamera(60, 1, 0.5, 100000);
  const dist = (maxDim / 2 / Math.tan((camera.fov * Math.PI) / 360)) * 1.6;
  camera.position.set(center.x + dist * 0.8, center.y + dist * 0.5, center.z + dist);
  camera.near = Math.max(0.1, dist / 1000);
  camera.far = Math.max(1000, dist * 20);
  camera.updateProjectionMatrix();
  const controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true;
  controls.dampingFactor = 0.08;
  controls.target.copy(center);
  controls.minDistance = Math.max(1, dist / 10);
  controls.maxDistance = dist * 6;
  controls.update();

  const resize = () => {
    const w = container.clientWidth || 1;
    const h = container.clientHeight || 1;
    renderer.setSize(w, h, false);
    camera.aspect = w / h;
    camera.updateProjectionMatrix();
  };
  resize();
  const ro = new ResizeObserver(resize);
  ro.observe(container);

  let raf = 0;
  const animate = () => {
    raf = requestAnimationFrame(animate);
    controls.update();
    renderer.render(scene, camera);
  };
  animate();

  return {
    setVertexPositions,
    setTextureMap,
    dispose() {
      cancelAnimationFrame(raf);
      ro.disconnect();
      controls.dispose();
      for (const b of buckets) {
        scene.remove(b.mesh);
        b.mesh.geometry.dispose();
        b.material.dispose();
      }
      for (const t of textures) t?.dispose();
      renderer.dispose();
      renderer.domElement.remove();
    },
  };
}
