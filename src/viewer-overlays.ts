import * as THREE from "three";
import { t } from "./i18n.ts";
import { styleValueAt } from "./lib/anim.ts";
import { toWorld } from "./lib/geometry.ts";
import { hullPositionsToWorld } from "./lib/hulls.ts";
import { soundHue } from "./lib/spatial-audio.ts";
import type { AmbientSound, Decal, HullMesh, LightSource, SpriteEnt, VolumeReport } from "./types/bsp-extra.ts";
import type { SpriteInfo } from "./types/formats.ts";
import type { MeshDetail } from "./types.ts";

/**
 * Camadas desenhadas por cima do mapa: hulls de colisão, volumes/zonas e a cena (luzes,
 * sprites, sons, decalques, rotas de trem). Tudo é criado sob demanda e fica num `Group`
 * próprio; `dispose()` solta geometria, material e texturas (nada de vazar GPU ao fechar).
 */

export type OverlayKey = "lights" | "sprites" | "sounds" | "decals" | "routes" | "volumes";

/** Texto do balão ao passar o mouse / clicar. */
export interface PickInfo {
  title: string;
  lines: string[];
}

export interface Pick {
  info: PickInfo;
  kind: "light" | "sprite" | "ambient" | "decal" | "volume";
  light?: LightSource;
  ambient?: AmbientSound;
}

/** Categoria da legenda de volumes (zona ou textura invisível). */
export interface VolumeCategory {
  key: string;
  label: string;
  color: string;
  count: number;
  on: boolean;
}

/** Tetos de desenho: acima disso o overlay para de adicionar e avisa. */
export const MAX_LIGHTS = 4000;
export const MAX_MARKERS = 1500;
export const MAX_SPRITE_INSTANCES = 600;
export const MAX_VOLUME_TRIANGLES = 200_000;
/** sons ambientes que a camada "Sons 3D" aceita (esfera + ícone + anel cada) */
export const MAX_SOUND3D = 300;

/** Como um som aparece na camada "Sons 3D" a cada quadro. */
export interface SoundVisual {
  /** tocando agora (o anel pulsa) */
  playing: boolean;
  /** 0..1, nível do sinal */
  level: number;
  /** arquivo ausente / codec sem suporte */
  unavailable: boolean;
  /** mudo ou fora da mixagem */
  silenced: boolean;
  /** destaque (som escolhido na lista) */
  selected: boolean;
}

interface SoundMarker {
  src: AmbientSound;
  icon: THREE.Sprite;
  iconMat: THREE.SpriteMaterial;
  ring: THREE.Sprite;
  ringMat: THREE.SpriteMaterial;
  fill: THREE.MeshBasicMaterial | null;
  wire: THREE.MeshBasicMaterial | null;
  baseScale: number;
}

const HULL_COLORS: Record<number, number> = { 1: 0x3fb950, 2: 0x4c7cf3, 3: 0xf0883e };

interface SpriteFrameTex {
  tex: THREE.Texture;
  w: number;
  h: number;
}

interface SpriteBank {
  frames: SpriteFrameTex[];
  additive: boolean;
  alphaTest: boolean;
  /** quadros por segundo quando a entidade não diz */
  fps: number;
}

interface SpriteInst {
  ent: SpriteEnt;
  sprite: THREE.Sprite;
  material: THREE.SpriteMaterial;
  frameCount: number;
}

/** Cor `0..255` do mapa -> cor do Three.js (a entrada é sRGB, como na tela do jogo). */
export const rgb01 = (c: readonly number[]): THREE.Color =>
  new THREE.Color().setRGB((c[0] ?? 0) / 255, (c[1] ?? 0) / 255, (c[2] ?? 0) / 255, THREE.SRGBColorSpace);

function iconTexture(draw: (g: CanvasRenderingContext2D) => void): THREE.CanvasTexture {
  const canvas = document.createElement("canvas");
  canvas.width = 64;
  canvas.height = 64;
  const g = canvas.getContext("2d");
  if (g) draw(g);
  const tex = new THREE.CanvasTexture(canvas);
  tex.colorSpace = THREE.SRGBColorSpace;
  return tex;
}

function disc(g: CanvasRenderingContext2D, color: string): void {
  g.fillStyle = "rgba(13,17,23,0.82)";
  g.beginPath();
  g.arc(32, 32, 29, 0, Math.PI * 2);
  g.fill();
  g.lineWidth = 3;
  g.strokeStyle = color;
  g.stroke();
}

/** Ícone de alto-falante (ambient_generic). */
function speakerIcon(): THREE.CanvasTexture {
  return iconTexture((g) => {
    disc(g, "#58a6ff");
    g.fillStyle = "#e6edf3";
    g.beginPath();
    g.moveTo(17, 27);
    g.lineTo(26, 27);
    g.lineTo(36, 19);
    g.lineTo(36, 45);
    g.lineTo(26, 37);
    g.lineTo(17, 37);
    g.closePath();
    g.fill();
    g.strokeStyle = "#e6edf3";
    g.lineWidth = 3;
    g.beginPath();
    g.arc(37, 32, 8, -0.9, 0.9);
    g.stroke();
    g.beginPath();
    g.arc(37, 32, 14, -0.9, 0.9);
    g.stroke();
  });
}

/** Anel branco fino (pulso do som tocando); a cor vem do material do sprite. */
function ringIcon(): THREE.CanvasTexture {
  return iconTexture((g) => {
    g.lineWidth = 4;
    g.strokeStyle = "#ffffff";
    g.beginPath();
    g.arc(32, 32, 28, 0, Math.PI * 2);
    g.stroke();
  });
}

/** Ícone de decalque (infodecal): quadrado com cantos marcados. */
function decalIcon(): THREE.CanvasTexture {
  return iconTexture((g) => {
    disc(g, "#d29922");
    g.strokeStyle = "#e6edf3";
    g.lineWidth = 3;
    g.strokeRect(20, 20, 24, 24);
    g.beginPath();
    g.moveTo(20, 32);
    g.lineTo(44, 32);
    g.moveTo(32, 20);
    g.lineTo(32, 44);
    g.stroke();
  });
}

/** Ícone de reserva de sprite (enquanto o `.spr` carrega, ou se não existir). */
function sparkIcon(): THREE.CanvasTexture {
  return iconTexture((g) => {
    disc(g, "#f2c14e");
    g.strokeStyle = "#f2c14e";
    g.lineWidth = 3;
    g.beginPath();
    for (let i = 0; i < 4; i++) {
      const a = (i * Math.PI) / 4;
      g.moveTo(32 - Math.cos(a) * 18, 32 - Math.sin(a) * 18);
      g.lineTo(32 + Math.cos(a) * 18, 32 + Math.sin(a) * 18);
    }
    g.stroke();
  });
}

const fmtRgb = (c: readonly number[]) => c.map((n) => Math.round(n)).join(" ");

export class Overlays {
  readonly group = new THREE.Group();
  /** raio base dos marcadores (unidades do mapa) */
  private readonly unit: number;

  // ---- hulls
  private readonly hullGroups = new Map<number, THREE.Group>();
  private hullShared: { fill: Map<number, THREE.MeshBasicMaterial>; wire: Map<number, THREE.MeshBasicMaterial> } = {
    fill: new Map(),
    wire: new Map(),
  };

  // ---- volumes
  private volumeGroup: THREE.Group | null = null;
  private volumeMaterials: THREE.Material[] = [];
  private readonly categories = new Map<string, VolumeCategory & { objects: THREE.Object3D[] }>();
  private volumesTruncated = false;
  private volumesVisible = false;

  // ---- cena
  private readonly groups: Record<Exclude<OverlayKey, "volumes">, THREE.Group> = {
    lights: new THREE.Group(),
    sprites: new THREE.Group(),
    sounds: new THREE.Group(),
    decals: new THREE.Group(),
    routes: new THREE.Group(),
  };
  private lightMesh: THREE.InstancedMesh | null = null;
  private lights: LightSource[] = [];
  private lightColors: THREE.Color[] = [];
  private lightStyleIdx: number[] = [];
  private lastLightFrame = -1;
  private styleDefsSeen: readonly { id: number; values: number[]; fps: number }[] | null = null;
  private styleById = new Map<number, { id: number; values: number[]; fps: number }>();
  private readonly spriteInsts: SpriteInst[] = [];
  private readonly spriteBanks = new Map<string, SpriteBank>();
  private readonly spriteRequested = new Set<string>();
  private readonly icons: { speaker: THREE.CanvasTexture; decal: THREE.CanvasTexture; spark: THREE.CanvasTexture };
  private readonly disposables: { dispose(): void }[] = [];
  private readonly pickables: THREE.Object3D[] = [];
  private readonly visibility: Record<OverlayKey, boolean> = {
    lights: false,
    sprites: false,
    sounds: false,
    decals: false,
    routes: false,
    volumes: false,
  };
  // ---- sons 3D (alcance, ícone e pulso por som)
  private readonly ambients: AmbientSound[] = [];
  private soundGroup: THREE.Group | null = null;
  private soundMarkers: SoundMarker[] = [];
  private soundVisible = false;
  /** esfera de raio do som selecionado */
  private readonly radiusBall: THREE.Mesh;
  /** quantidade de marcadores por camada (para a interface esconder o que não existe) */
  readonly counts: Record<OverlayKey, number> = { lights: 0, sprites: 0, sounds: 0, decals: 0, routes: 0, volumes: 0 };

  constructor(mesh: MeshDetail, unit: number) {
    this.unit = unit;
    this.group.name = "overlays";
    this.icons = { speaker: speakerIcon(), decal: decalIcon(), spark: sparkIcon() };
    this.disposables.push(this.icons.speaker, this.icons.decal, this.icons.spark);
    for (const g of Object.values(this.groups)) {
      g.visible = false;
      this.group.add(g);
    }
    const ballMat = new THREE.MeshBasicMaterial({ color: 0x58a6ff, wireframe: true, transparent: true, opacity: 0.35, depthWrite: false, fog: false });
    this.radiusBall = new THREE.Mesh(new THREE.SphereGeometry(1, 24, 14), ballMat);
    this.radiusBall.visible = false;
    this.disposables.push(this.radiusBall.geometry, ballMat);
    this.group.add(this.radiusBall);
    this.buildScene(mesh);
  }

  // ------------------------------------------------------------------ cena

  private buildScene(mesh: MeshDetail): void {
    const entities = mesh.scene.entities;
    const lights: LightSource[] = [];
    let markers = 0;
    for (const e of entities) {
      if (e.type === "light") {
        if (lights.length < MAX_LIGHTS) lights.push(e);
      } else if (e.type === "ambient") {
        if (this.ambients.length < MAX_SOUND3D) this.ambients.push(e);
        if (markers++ < MAX_MARKERS) this.addIconMarker("sounds", this.icons.speaker, e, e.origin, 2.2);
      } else if (e.type === "decal") {
        if (markers++ < MAX_MARKERS) this.addIconMarker("decals", this.icons.decal, e, e.origin, 1.6);
      } else if (e.type === "sprite") {
        if (this.spriteInsts.length < MAX_SPRITE_INSTANCES) this.addSprite(e);
      }
    }
    this.buildLights(lights);
    this.buildRoutes(mesh);
    this.counts.lights = lights.length;
    this.counts.sprites = this.spriteInsts.length;
    this.counts.sounds = this.groups.sounds.children.length;
    this.counts.decals = this.groups.decals.children.length;
    this.counts.routes = mesh.scene.routes.filter((r) => r.points.length > 1).length;
  }

  private buildLights(lights: LightSource[]): void {
    if (!lights.length) return;
    this.lights = lights;
    const geo = new THREE.SphereGeometry(1, 10, 8);
    const mat = new THREE.MeshBasicMaterial({ color: 0xffffff, fog: false });
    const inst = new THREE.InstancedMesh(geo, mat, lights.length);
    inst.instanceMatrix.setUsage(THREE.StaticDrawUsage);
    const m = new THREE.Matrix4();
    const q = new THREE.Quaternion();
    const s = new THREE.Vector3();
    lights.forEach((l, i) => {
      const [x, y, z] = toWorld(l.origin[0], l.origin[1], l.origin[2]);
      // brilho maior = marcador um pouco maior (sem exagerar)
      const r = this.unit * Math.max(0.7, Math.min(1.8, Math.sqrt(Math.max(l.brightness, 1) / 200)));
      m.compose(new THREE.Vector3(x, y, z), q, s.set(r, r, r));
      inst.setMatrixAt(i, m);
      const c = rgb01(l.color);
      this.lightColors.push(c);
      inst.setColorAt(i, c);
      this.lightStyleIdx.push(l.style);
    });
    inst.instanceMatrix.needsUpdate = true;
    if (inst.instanceColor) inst.instanceColor.setUsage(THREE.DynamicDrawUsage);
    inst.frustumCulled = false;
    this.lightMesh = inst;
    this.groups.lights.add(inst);
    this.disposables.push(geo, mat, inst);
    this.pickables.push(inst);
  }

  private addIconMarker(
    layer: "sounds" | "decals",
    icon: THREE.Texture,
    data: AmbientSound | Decal,
    origin: readonly number[],
    sizeK: number,
  ): void {
    const mat = new THREE.SpriteMaterial({ map: icon, depthTest: false, transparent: true, fog: false });
    const sprite = new THREE.Sprite(mat);
    const [x, y, z] = toWorld(origin[0] ?? 0, origin[1] ?? 0, origin[2] ?? 0);
    sprite.position.set(x, y, z);
    sprite.scale.setScalar(this.unit * sizeK * 2);
    sprite.renderOrder = 20;
    sprite.userData.pick = data;
    this.groups[layer].add(sprite);
    this.disposables.push(mat);
    this.pickables.push(sprite);
  }

  private addSprite(ent: SpriteEnt): void {
    const material = new THREE.SpriteMaterial({ map: this.icons.spark, transparent: true, depthWrite: false, fog: false });
    const sprite = new THREE.Sprite(material);
    const [x, y, z] = toWorld(ent.origin[0], ent.origin[1], ent.origin[2]);
    sprite.position.set(x, y, z);
    sprite.scale.setScalar(this.unit * 3);
    sprite.renderOrder = 15;
    sprite.userData.pick = ent;
    this.groups.sprites.add(sprite);
    this.spriteInsts.push({ ent, sprite, material, frameCount: 1 });
    this.disposables.push(material);
    this.pickables.push(sprite);
    if (ent.model) this.spriteRequested.add(ent.model);
  }

  private buildRoutes(mesh: MeshDetail): void {
    for (const r of mesh.scene.routes) {
      if (r.points.length < 2) continue;
      const pts: number[] = [];
      for (const p of r.points) pts.push(...toWorld(p[0], p[1], p[2]));
      if (r.looped) pts.push(...toWorld(r.points[0]![0], r.points[0]![1], r.points[0]![2]));
      const geo = new THREE.BufferGeometry();
      geo.setAttribute("position", new THREE.Float32BufferAttribute(pts, 3));
      const color = r.broken ? 0xf85149 : 0xf2c14e;
      const mat = new THREE.LineBasicMaterial({ color, depthTest: false, transparent: true, opacity: 0.9, fog: false });
      const line = new THREE.Line(geo, mat);
      line.renderOrder = 18;
      line.frustumCulled = false;
      const dotsGeo = new THREE.BufferGeometry();
      dotsGeo.setAttribute("position", new THREE.Float32BufferAttribute(pts.slice(0, r.points.length * 3), 3));
      const dotsMat = new THREE.PointsMaterial({ color, size: Math.max(4, this.unit * 1.4), sizeAttenuation: true, depthTest: false, fog: false });
      const dots = new THREE.Points(dotsGeo, dotsMat);
      dots.renderOrder = 19;
      dots.frustumCulled = false;
      this.groups.routes.add(line, dots);
      this.disposables.push(geo, mat, dotsGeo, dotsMat);
    }
  }

  /** `.spr` que as entidades de sprite pedem (para a interface carregar com `load_sprite`). */
  spriteModels(): string[] {
    return [...this.spriteRequested];
  }

  /** Entrega o `.spr` carregado (ou `null` se falhou: o ícone de reserva fica). */
  setSpriteInfo(model: string, info: SpriteInfo | null): void {
    if (!info) return;
    const frames: SpriteFrameTex[] = [];
    for (const f of info.frames) {
      for (const img of f.images) {
        if (frames.length >= 64) break;
        const tex = new THREE.TextureLoader().load(img.png);
        tex.colorSpace = THREE.SRGBColorSpace;
        frames.push({ tex, w: img.width, h: img.height });
        this.disposables.push(tex);
      }
    }
    if (!frames.length) return;
    const fps = info.total_duration > 0 ? frames.length / info.total_duration : 10;
    this.spriteBanks.set(model, { frames, additive: info.additive || info.tex_format === "additive", alphaTest: info.tex_format === "alpha_test", fps });
    for (const inst of this.spriteInsts) {
      if (inst.ent.model !== model) continue;
      const bank = this.spriteBanks.get(model)!;
      const first = bank.frames[0]!;
      const e = inst.ent;
      const mode = e.render.mode;
      const additive = bank.additive || mode === 5 || mode === 3;
      inst.material.map = first.tex;
      inst.material.blending = additive ? THREE.AdditiveBlending : THREE.NormalBlending;
      inst.material.alphaTest = bank.alphaTest ? 0.5 : 0;
      // sem renderamt no mapa (0) o sprite aparece inteiro; com modo de render vale amount / 255
      inst.material.opacity = mode !== 0 && e.render.amount > 0 ? e.render.amount / 255 : 1;
      const tint = e.render.color;
      if (mode !== 0 && (tint[0] || tint[1] || tint[2])) inst.material.color.copy(rgb01(tint));
      const k = e.scale > 0 ? e.scale : 1;
      inst.sprite.scale.set(first.w * k, first.h * k, 1);
      inst.frameCount = bank.frames.length;
      inst.material.needsUpdate = true;
    }
  }

  // ------------------------------------------------------------------ visibilidade

  setVisible(key: OverlayKey, on: boolean): void {
    this.visibility[key] = on;
    if (key === "volumes") {
      this.volumesVisible = on;
      this.refreshVolumeVisibility();
    } else {
      this.groups[key].visible = on;
    }
    if (!on) this.radiusBall.visible = false;
  }

  isVisible(key: OverlayKey): boolean {
    return this.visibility[key];
  }

  // ------------------------------------------------------------------ animação por quadro

  /**
   * `time` em segundos. `lightsAnimated` aplica o padrão do estilo à cor do marcador
   * (a 10 Hz); `lightsOff` são os estilos ligáveis apagados.
   */
  update(
    time: number,
    styleDefs: readonly { id: number; values: number[]; fps: number }[],
    lightsAnimated: boolean,
    lightsOff: ReadonlySet<number>,
  ): void {
    if (this.groups.sprites.visible) {
      for (const inst of this.spriteInsts) {
        if (inst.frameCount < 2) continue;
        const bank = this.spriteBanks.get(inst.ent.model ?? "");
        if (!bank) continue;
        const fps = inst.ent.framerate > 0 ? inst.ent.framerate : bank.fps;
        const idx = Math.floor(time * fps) % bank.frames.length;
        const f = bank.frames[idx]!;
        if (inst.material.map !== f.tex) inst.material.map = f.tex;
      }
    }
    if (this.lightMesh && this.groups.lights.visible) {
      const frame = Math.floor(time * 10);
      if (frame !== this.lastLightFrame) {
        this.lastLightFrame = frame;
        this.refreshLightColors(time, styleDefs, lightsAnimated, lightsOff);
      }
    }
    if (this.radiusBall.visible) {
      this.radiusBall.scale.setScalar((this.radiusBall.userData.radius as number) * (1 + Math.sin(time * 3) * 0.015));
    }
  }

  /** Força a releitura das cores das luzes (liga/desliga de um estilo). */
  invalidateLights(): void {
    this.lastLightFrame = -1;
  }

  private refreshLightColors(
    time: number,
    defs: readonly { id: number; values: number[]; fps: number }[],
    animated: boolean,
    off: ReadonlySet<number>,
  ): void {
    const inst = this.lightMesh!;
    if (this.styleDefsSeen !== defs) {
      this.styleDefsSeen = defs;
      this.styleById = new Map(defs.map((d) => [d.id, d]));
    }
    const byId = this.styleById;
    const c = new THREE.Color();
    for (let i = 0; i < this.lights.length; i++) {
      const style = this.lightStyleIdx[i]!;
      let k = 1;
      if (off.has(style)) k = 0.12;
      else if (animated && style !== 0) {
        const def = byId.get(style);
        if (def) k = Math.min(1, styleValueAt(def, time));
      }
      c.copy(this.lightColors[i]!).multiplyScalar(0.25 + 0.75 * k);
      inst.setColorAt(i, c);
    }
    if (inst.instanceColor) inst.instanceColor.needsUpdate = true;
  }

  // ------------------------------------------------------------------ hulls

  /** Acrescenta uma janela do hull `hull` (1..3). Cada janela é uma geometria própria. */
  addHullWindow(hull: number, win: HullMesh): void {
    if (!win.indices.length || !win.positions.length) return;
    let g = this.hullGroups.get(hull);
    if (!g) {
      g = new THREE.Group();
      g.name = `hull${hull}`;
      this.hullGroups.set(hull, g);
      this.group.add(g);
    }
    const color = HULL_COLORS[hull] ?? 0xffffff;
    let fill = this.hullShared.fill.get(hull);
    let wire = this.hullShared.wire.get(hull);
    if (!fill || !wire) {
      fill = new THREE.MeshBasicMaterial({ color, transparent: true, opacity: 0.1, side: THREE.DoubleSide, depthWrite: false, fog: false });
      wire = new THREE.MeshBasicMaterial({ color, wireframe: true, transparent: true, opacity: 0.5, depthWrite: false, fog: false });
      this.hullShared.fill.set(hull, fill);
      this.hullShared.wire.set(hull, wire);
      this.disposables.push(fill, wire);
    }
    const geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.BufferAttribute(hullPositionsToWorld(win.positions), 3));
    geo.setIndex(new THREE.BufferAttribute(Uint32Array.from(win.indices), 1));
    geo.computeBoundingSphere();
    const a = new THREE.Mesh(geo, fill);
    const b = new THREE.Mesh(geo, wire);
    a.renderOrder = 5;
    b.renderOrder = 6;
    g.add(a, b);
    this.disposables.push(geo);
  }

  clearHull(hull: number): void {
    const g = this.hullGroups.get(hull);
    if (!g) return;
    // geometrias de janela são compartilhadas entre o preenchimento e o aramado
    const geos = new Set<THREE.BufferGeometry>();
    for (const o of g.children) geos.add((o as THREE.Mesh).geometry);
    for (const geo of geos) geo.dispose();
    this.group.remove(g);
    this.hullGroups.delete(hull);
    // os materiais ficam para a próxima janela; a geometria já foi liberada
  }

  setHullVisible(hull: number, on: boolean): void {
    const g = this.hullGroups.get(hull);
    if (g) g.visible = on;
  }

  hullTriangles(hull: number): number {
    let n = 0;
    for (const o of this.hullGroups.get(hull)?.children ?? []) {
      const m = o as THREE.Mesh;
      if (m.material === this.hullShared.fill.get(hull)) n += (m.geometry.index?.count ?? 0) / 3;
    }
    return n;
  }

  // ------------------------------------------------------------------ volumes

  /** Constrói os volumes e as superfícies invisíveis. Chamar de novo troca o conteúdo. */
  setVolumes(report: VolumeReport): void {
    this.clearVolumes();
    const g = new THREE.Group();
    g.name = "volumes";
    g.visible = this.volumesVisible;
    this.volumeGroup = g;
    this.group.add(g);

    const legend = new Map(report.legend.map((l) => [l.kind as string, l]));
    const matFor = new Map<string, { fill: THREE.MeshBasicMaterial; edge: THREE.LineBasicMaterial }>();
    const matsOf = (key: string, color: string) => {
      let m = matFor.get(key);
      if (!m) {
        const c = new THREE.Color(color);
        m = {
          fill: new THREE.MeshBasicMaterial({ color: c, transparent: true, opacity: 0.2, side: THREE.DoubleSide, depthWrite: false, fog: false }),
          edge: new THREE.LineBasicMaterial({ color: c, transparent: true, opacity: 0.85, fog: false }),
        };
        matFor.set(key, m);
        this.volumeMaterials.push(m.fill, m.edge);
      }
      return m;
    };
    const category = (key: string, label: string, color: string) => {
      let c = this.categories.get(key);
      if (!c) {
        c = { key, label, color, count: 0, on: true, objects: [] };
        this.categories.set(key, c);
      }
      return c;
    };

    let tris = 0;
    this.volumesTruncated = report.truncated;
    const add = (key: string, label: string, color: string, positions: number[], pick: PickInfo | null) => {
      const n = Math.floor(positions.length / 9);
      if (!n) return;
      if (tris + n > MAX_VOLUME_TRIANGLES) {
        this.volumesTruncated = true;
        return;
      }
      tris += n;
      const cat = category(key, label, color);
      const mats = matsOf(key, color);
      const world = new Float32Array(n * 9);
      for (let i = 0; i < n * 3; i++) {
        const w = toWorld(positions[i * 3]!, positions[i * 3 + 1]!, positions[i * 3 + 2]!);
        world[i * 3] = w[0];
        world[i * 3 + 1] = w[1];
        world[i * 3 + 2] = w[2];
      }
      const geo = new THREE.BufferGeometry();
      geo.setAttribute("position", new THREE.BufferAttribute(world, 3));
      geo.computeBoundingSphere();
      const mesh = new THREE.Mesh(geo, mats.fill);
      mesh.renderOrder = 7;
      if (pick) mesh.userData.pick = pick;
      mesh.userData.cat = key;
      g.add(mesh);
      cat.objects.push(mesh);
      cat.count++;
      this.disposables.push(geo);
      if (n <= 120) {
        // contorno nítido nas caixas pequenas (o preenchimento a 20 % some de longe)
        const edges = new THREE.EdgesGeometry(geo, 25);
        const lines = new THREE.LineSegments(edges, mats.edge);
        lines.renderOrder = 8;
        g.add(lines);
        cat.objects.push(lines);
        this.disposables.push(edges);
      }
      if (pick) this.pickables.push(mesh);
    };

    for (const v of report.volumes) {
      const l = legend.get(v.kind);
      const color = l?.color ?? v.color;
      const label = zoneLabel(v.kind, l?.label ?? v.kind_label);
      const lines = [`${label} · ${v.classname}`];
      if (v.targetname) lines.push(t("vw.tip.name", v.targetname));
      if (v.target) lines.push(t("vw.tip.target", v.target));
      if (v.message) lines.push(t("vw.tip.message", v.message));
      add(`zone:${v.kind}`, label, color, v.positions, { title: v.label || v.classname, lines });
    }
    for (const s of report.invisible) {
      add(`inv:${s.texture}`, s.texture, s.color, s.positions, { title: s.texture, lines: [t("vw.tip.invisible", s.faces)] });
    }
    // legenda na ordem do backend; texturas invisíveis depois
    this.counts.volumes = [...this.categories.values()].reduce((n, c) => n + c.count, 0);
    this.refreshVolumeVisibility();
    this.rebuildPickables();
  }

  volumeCategories(): VolumeCategory[] {
    return [...this.categories.values()].map(({ key, label, color, count, on }) => ({ key, label, color, count, on }));
  }

  get volumesWereTruncated(): boolean {
    return this.volumesTruncated;
  }

  setVolumeCategory(key: string, on: boolean): void {
    const c = this.categories.get(key);
    if (!c) return;
    c.on = on;
    for (const o of c.objects) o.visible = on;
  }

  private refreshVolumeVisibility(): void {
    if (this.volumeGroup) this.volumeGroup.visible = this.volumesVisible;
  }

  private clearVolumes(): void {
    if (this.volumeGroup) {
      this.group.remove(this.volumeGroup);
      this.volumeGroup = null;
    }
    // geometria dos volumes antigos: sai da lista de descarte junto com os materiais
    for (const c of this.categories.values()) {
      for (const o of c.objects) {
        const m = o as THREE.Mesh;
        m.geometry?.dispose();
      }
    }
    for (const m of this.volumeMaterials) m.dispose();
    this.volumeMaterials.length = 0;
    this.categories.clear();
    this.counts.volumes = 0;
    this.rebuildPickables();
  }

  private rebuildPickables(): void {
    // mantém só o que ainda está na árvore (volumes trocados saem da lista)
    const live = (o: THREE.Object3D): boolean => {
      let p: THREE.Object3D | null = o;
      while (p) {
        if (p === this.group) return true;
        p = p.parent;
      }
      return false;
    };
    for (let i = this.pickables.length - 1; i >= 0; i--) if (!live(this.pickables[i]!)) this.pickables.splice(i, 1);
  }

  // ------------------------------------------------------------------ pick

  /** Primeiro objeto visível sob o raio, ou `null`. */
  pick(raycaster: THREE.Raycaster): Pick | null {
    const shown = this.pickables.filter((o) => isShown(o, this.group) && this.categories.get(o.userData.cat as string)?.on !== false);
    // marcadores (luz, som, sprite, decalque) têm prioridade sobre volumes: o ícone aparece
    // por cima de tudo, então o clique nele não pode cair na zona que está na frente
    const markers = shown.filter((o) => o.userData.cat === undefined);
    const volumes = shown.filter((o) => o.userData.cat !== undefined);
    const hit = (markers.length ? raycaster.intersectObjects(markers, false)[0] : undefined) ?? (volumes.length ? raycaster.intersectObjects(volumes, false)[0] : undefined);
    if (!hit) return null;
    if (hit.object === this.lightMesh && hit.instanceId !== undefined) {
      const l = this.lights[hit.instanceId];
      return l ? { kind: "light", light: l, info: lightInfo(l) } : null;
    }
    const data = hit.object.userData.pick as unknown;
    if (!data) return null;
    if (isPickInfo(data)) return { kind: "volume", info: data };
    const d = data as AmbientSound | SpriteEnt | Decal;
    if (d.type === "ambient") return { kind: "ambient", ambient: d, info: ambientInfo(d) };
    if (d.type === "sprite") return { kind: "sprite", info: spriteInfo(d) };
    return { kind: "decal", info: { title: d.classname, lines: [t("vw.tip.decal", d.texture ?? "—"), d.brush ? t("vw.tip.decalBrush") : t("vw.tip.decalPoint")] } };
  }

  /** Mostra o raio do som (esfera aramada) ou esconde com `null`. */
  showRadius(ambient: AmbientSound | null): void {
    if (!ambient || ambient.radius === null || !(ambient.radius > 0)) {
      this.radiusBall.visible = false;
      return;
    }
    const [x, y, z] = toWorld(ambient.origin[0], ambient.origin[1], ambient.origin[2]);
    this.radiusBall.position.set(x, y, z);
    this.radiusBall.userData.radius = ambient.radius;
    this.radiusBall.scale.setScalar(ambient.radius);
    this.radiusBall.visible = true;
  }

  // ------------------------------------------------------------------ sons 3D

  /** Os `ambient_generic` do mapa na ordem em que a camada "Sons 3D" os numera (até `MAX_SOUND3D`). */
  ambientSources(): readonly AmbientSound[] {
    return this.ambients;
  }

  /** Liga/desliga o desenho de alcance, ícone e pulso dos sons; os objetos nascem na primeira vez. */
  setSoundVisuals(on: boolean): void {
    this.soundVisible = on;
    if (on && !this.soundGroup) this.buildSoundVisuals();
    if (this.soundGroup) this.soundGroup.visible = on;
  }

  private buildSoundVisuals(): void {
    const group = new THREE.Group();
    group.name = "sound3d";
    this.group.add(group);
    this.soundGroup = group;
    const sphere = new THREE.SphereGeometry(1, 28, 16);
    const ringTex = ringIcon();
    this.disposables.push(sphere, ringTex);
    this.ambients.forEach((src, id) => {
      const color = new THREE.Color().setHSL(soundHue(id) / 360, 0.75, 0.58);
      const [x, y, z] = toWorld(src.origin[0], src.origin[1], src.origin[2]);
      let fill: THREE.MeshBasicMaterial | null = null;
      let wire: THREE.MeshBasicMaterial | null = null;
      if (src.radius !== null && src.radius > 0) {
        fill = new THREE.MeshBasicMaterial({ color, transparent: true, opacity: 0.045, side: THREE.BackSide, depthWrite: false, fog: false });
        wire = new THREE.MeshBasicMaterial({ color, wireframe: true, transparent: true, opacity: 0.22, depthWrite: false, fog: false });
        const a = new THREE.Mesh(sphere, fill);
        const b = new THREE.Mesh(sphere, wire);
        for (const m of [a, b]) {
          m.position.set(x, y, z);
          m.scale.setScalar(src.radius);
          m.renderOrder = 5;
          m.raycast = () => undefined; // o alcance não rouba o clique dos ícones
          group.add(m);
        }
        this.disposables.push(fill, wire);
      }
      const iconMat = new THREE.SpriteMaterial({ map: this.icons.speaker, color, depthTest: false, transparent: true, fog: false });
      const icon = new THREE.Sprite(iconMat);
      const baseScale = this.unit * 2.2 * 2;
      icon.position.set(x, y, z);
      icon.scale.setScalar(baseScale);
      icon.renderOrder = 22;
      icon.userData.pick = src;
      const ringMat = new THREE.SpriteMaterial({ map: ringTex, color, depthTest: false, transparent: true, opacity: 0, fog: false });
      const ring = new THREE.Sprite(ringMat);
      ring.position.set(x, y, z);
      ring.scale.setScalar(baseScale);
      ring.renderOrder = 21;
      ring.visible = false;
      group.add(icon, ring);
      this.pickables.push(icon);
      this.disposables.push(iconMat, ringMat);
      this.soundMarkers.push({ src, icon, iconMat, ring, ringMat, fill, wire, baseScale });
    });
  }

  /** Anima o pulso e o brilho do alcance conforme o estado de cada som (índice = id do som). */
  updateSoundVisuals(visuals: readonly (SoundVisual | undefined)[], time: number): void {
    if (!this.soundVisible) return;
    this.soundMarkers.forEach((m, id) => {
      const v = visuals[id];
      const playing = !!v?.playing;
      const dim = !!v && (v.unavailable || v.silenced);
      m.iconMat.opacity = v?.unavailable ? 0.4 : dim ? 0.55 : 1;
      m.icon.scale.setScalar(m.baseScale * (v?.selected ? 1.35 : 1));
      if (m.wire) m.wire.opacity = v?.selected ? 0.6 : playing ? 0.32 : 0.18;
      if (m.fill) m.fill.opacity = v?.selected ? 0.12 : playing ? 0.075 : 0.04;
      m.ring.visible = playing;
      if (playing) {
        const phase = (time * 1.1 + id * 0.37) % 1;
        const lvl = v ? v.level : 0;
        m.ring.scale.setScalar(m.baseScale * (1 + phase * 2.4));
        m.ringMat.opacity = (1 - phase) * (0.35 + 0.65 * lvl);
        // o ícone "bate" com o nível do sinal
        m.icon.scale.setScalar(m.baseScale * (v?.selected ? 1.35 : 1) * (1 + lvl * 0.35));
      }
    });
  }

  // ------------------------------------------------------------------ descarte

  dispose(): void {
    for (const c of this.categories.values()) {
      for (const o of c.objects) (o as THREE.Mesh).geometry?.dispose();
    }
    for (const d of this.disposables) d.dispose();
    this.disposables.length = 0;
    for (const m of this.volumeMaterials) m.dispose();
    this.volumeMaterials.length = 0;
    this.categories.clear();
    this.pickables.length = 0;
    this.spriteInsts.length = 0;
    this.soundMarkers.length = 0;
    this.soundGroup = null;
    this.spriteBanks.clear();
    this.hullGroups.clear();
    this.group.removeFromParent();
  }
}

function isPickInfo(d: unknown): d is PickInfo {
  return typeof d === "object" && d !== null && "title" in d && "lines" in d;
}

function isShown(o: THREE.Object3D, root: THREE.Object3D): boolean {
  for (let p: THREE.Object3D | null = o; p && p !== root.parent; p = p.parent) if (!p.visible) return false;
  return true;
}

const ZONE_KEYS = new Set(["trigger", "buy_zone", "bomb_target", "hostage_rescue", "vip_safety", "escape_zone", "ladder", "water"]);

/** Nome da zona no idioma atual (o do backend é só o plano B). */
export function zoneLabel(kind: string, fallback: string): string {
  return ZONE_KEYS.has(kind) ? t(`vw.zone.${kind}`) : fallback;
}

function lightInfo(l: LightSource): PickInfo {
  const lines = [t("vw.tip.lightColor", fmtRgb(l.color), Math.round(l.brightness))];
  if (l.style !== 0) lines.push(t("vw.tip.lightStyle", l.style, l.pattern ?? "—"));
  if (l.switchable) lines.push(t("vw.tip.lightSwitch"));
  return { title: l.spot ? t("vw.tip.spot") : t("vw.tip.light"), lines: l.targetname ? [t("vw.tip.name", l.targetname), ...lines] : lines };
}

function ambientInfo(a: AmbientSound): PickInfo {
  return {
    title: t("vw.tip.sound"),
    lines: [
      t("vw.tip.soundFile", a.sound ?? "—"),
      a.radius === null ? t("vw.tip.radiusAll") : t("vw.tip.radius", Math.round(a.radius)),
      t("vw.tip.volume", Math.round(a.volume * 100)),
      ...(a.start_silent ? [t("vw.tip.silent")] : []),
    ],
  };
}

function spriteInfo(s: SpriteEnt): PickInfo {
  return {
    title: s.glow ? "env_glow" : "env_sprite",
    lines: [t("vw.tip.spriteFile", s.model ?? "—"), t("vw.tip.spriteRender", s.render.mode_name, Math.round(s.render.amount))],
  };
}
