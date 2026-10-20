import * as THREE from "three";
import { MdlRig, addModelLights, decodeTextures, emptyTextures, restBounds, type Look, type Model, type TextureSet } from "./mdl-rig.ts";

/**
 * Estúdio das miniaturas de skin: UM `WebGLRenderer` compartilhado (e uma cena só) que desenha cada família
 * de skin e entrega o quadro a um canvas 2D de miniatura (`drawImage`). A geometria é a mesma pra todas as
 * famílias (`MdlRig`); cada família é só um jogo de materiais, criado sob demanda e guardado (`looks`).
 * Nunca há mais que este contexto WebGL, qualquer que seja o número de miniaturas.
 */
export class SkinStudio {
  private readonly renderer: THREE.WebGLRenderer;
  private readonly scene = new THREE.Scene();
  private readonly camera = new THREE.PerspectiveCamera(32, 1, 0.1, 10000);
  private rig: MdlRig | null = null;
  private textures: TextureSet = emptyTextures(0);
  private readonly looks = new Map<string, Look>();
  private center = new THREE.Vector3();
  private distance = 100;
  private swap = 0;
  private disposed = false;

  constructor(readonly size = 128) {
    this.renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, preserveDrawingBuffer: false });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.renderer.setClearColor(0x000000, 0);
    this.renderer.setSize(size, size, false);
    addModelLights(this.scene);
  }

  /** Pixels do desenho (lado do canvas de miniatura que casa com o estúdio). */
  get pixels(): number {
    return this.renderer.domElement.width;
  }

  /** Troca o modelo: geometria nova, texturas decodificadas, looks descartados. Resolve quando está pronto. */
  async setModel(model: Model, positions?: ArrayLike<number>): Promise<void> {
    const mine = ++this.swap;
    const textures = await decodeTextures(model);
    if (this.disposed || mine !== this.swap) {
      textures.dispose();
      return;
    }
    this.clear();
    const rig = new MdlRig(model);
    if (positions) rig.setVertexPositions(positions);
    this.scene.add(rig.root);
    this.rig = rig;
    this.textures = textures;
    const box = restBounds(model);
    box.getCenter(this.center);
    const radius = Math.max(box.getSize(new THREE.Vector3()).length() / 2, 1);
    this.distance = (radius / Math.sin((this.camera.fov * Math.PI) / 360)) * 1.05;
    this.camera.near = Math.max(0.05, this.distance / 200);
    this.camera.far = this.distance * 20;
    this.camera.updateProjectionMatrix();
  }

  /** Há um modelo pronto para desenhar? */
  get ready(): boolean {
    return this.rig !== null;
  }

  /**
   * Desenha a família `map` girada em `angle` (radianos) e copia o quadro para `target`.
   * O look da família é criado na primeira vez e reaproveitado.
   */
  draw(target: HTMLCanvasElement, map: readonly number[], angle: number): void {
    const rig = this.rig;
    if (!rig || this.disposed) return;
    const key = map.join(",");
    let look = this.looks.get(key);
    if (!look) {
      look = rig.addLook(map, this.textures);
      this.looks.set(key, look);
    }
    for (const l of this.looks.values()) l.group.visible = l === look;
    const elev = 0.32;
    this.camera.position.set(
      this.center.x + this.distance * Math.sin(angle) * Math.cos(elev),
      this.center.y + this.distance * Math.sin(elev),
      this.center.z + this.distance * Math.cos(angle) * Math.cos(elev),
    );
    this.camera.lookAt(this.center);
    this.renderer.render(this.scene, this.camera);
    const ctx = target.getContext("2d");
    if (!ctx) return;
    ctx.clearRect(0, 0, target.width, target.height);
    ctx.drawImage(this.renderer.domElement, 0, 0, target.width, target.height);
  }

  /** Quantos looks (jogos de materiais) estão vivos — para os testes e o diagnóstico. */
  get lookCount(): number {
    return this.looks.size;
  }

  private clear() {
    for (const l of this.looks.values()) l.dispose();
    this.looks.clear();
    this.rig?.dispose();
    this.rig = null;
    this.textures.dispose();
    this.textures = emptyTextures(0);
  }

  dispose(): void {
    this.disposed = true;
    this.swap++;
    this.clear();
    this.renderer.dispose();
    this.renderer.forceContextLoss(); // devolve o contexto na hora, sem esperar o coletor
  }
}
