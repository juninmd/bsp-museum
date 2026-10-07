import { invoke } from "@tauri-apps/api/core";
import { t } from "./i18n.ts";
import { mountModelViewer3D, type ModelViewer } from "./model-viewer.ts";
import type { MdlSeqFrames, MdlSummary } from "./types.ts";

/** floats por bone por quadro: posição (3) + quaternion xyzw (4) — mesmo layout de `mdl::POSE_STRIDE` */
const STRIDE = 7;

interface Pose {
  frames: number;
  bones: number;
  fps: number;
  looping: boolean;
  data: Float32Array;
}

/**
 * Visualizador de um `.mdl` isolado: cena orbitável própria (`model-viewer.ts`)
 * + a barra de ferramentas (sequência, tocar/pausar, quadro, velocidade, skin)
 * ligada por `bindAnimation`; `path` é o arquivo, pra pedir os quadros.
 */
export function mountModelViewer(container: HTMLElement, model: MdlSummary, path: string): { dispose(): void } {
  const viewer = mountModelViewer3D(container, model);
  const animation = bindAnimation(viewer, model, path);
  return {
    dispose() {
      animation.dispose();
      viewer.dispose();
    },
  };
}

/**
 * Animação por skinning na CPU: pra cada quadro o backend entrega a pose de
 * mundo (posição + quaternion) de cada bone; aqui interpolamos entre dois
 * quadros, montamos a matriz de cada bone e movemos os vértices
 * (`local_positions`, no espaço do bone dono) — mesma conta do SDK
 * (`pose[bone] * local`). Os quadros de cada sequência vêm sob demanda
 * (`load_sequence`) e ficam em cache enquanto o modelo está aberto.
 */
function bindAnimation(viewer: ModelViewer, model: MdlSummary, path: string): { dispose(): void } {
  const byId = <T extends HTMLElement>(id: string) => document.querySelector<T>(`#${id}`)!;
  const seqSel = byId<HTMLSelectElement>("res-sequence");
  const playBtn = byId<HTMLButtonElement>("res-play");
  const frameInput = byId<HTMLInputElement>("res-frame");
  const frameLabel = byId<HTMLElement>("res-frame-label");
  const speedSel = byId<HTMLSelectElement>("res-speed");
  const skinWrap = byId<HTMLElement>("res-skin-wrap");
  const skinSel = byId<HTMLSelectElement>("res-skin");
  const note = byId<HTMLElement>("res-anim-note");

  const ctl = new AbortController();
  const on = { signal: ctl.signal };
  const cache = new Map<number, Pose>();
  const out = new Float32Array(model.local_positions.length);
  const mats = new Float32Array(model.num_bones * 12); // por bone: 3x3 (linha a linha) + posição
  let pose: Pose | null = null;
  let frame = 0;
  let playing = false;
  let speed = 1;
  let token = 0; // descarta resposta de uma sequência que já não é a escolhida
  let raf = 0;
  let last = performance.now();

  function setPlaying(value: boolean) {
    playing = value;
    playBtn.textContent = value ? t("mdl.pause") : t("mdl.play");
    playBtn.setAttribute("aria-label", value ? t("mdl.pauseAria") : t("mdl.playAria"));
  }

  function setEnabled(value: boolean) {
    playBtn.disabled = !value;
    frameInput.disabled = !value;
  }

  /** pose interpolada entre os quadros `floor(f)` e o seguinte -> vértices -> cena */
  function applyFrame(f: number) {
    if (!pose || pose.bones !== model.num_bones) return;
    const f0 = Math.max(0, Math.min(Math.floor(f), pose.frames - 1));
    const f1 = Math.min(f0 + 1, pose.frames - 1);
    const t = Math.max(0, Math.min(f - f0, 1));
    const d = pose.data;
    for (let b = 0; b < pose.bones; b++) {
      const a = (f0 * pose.bones + b) * STRIDE;
      const c = (f1 * pose.bones + b) * STRIDE;
      // nlerp com o lado mais curto: entre dois quadros seguidos o ângulo é pequeno
      const sign = d[a + 3]! * d[c + 3]! + d[a + 4]! * d[c + 4]! + d[a + 5]! * d[c + 5]! + d[a + 6]! * d[c + 6]! < 0 ? -1 : 1;
      let x = d[a + 3]! + (sign * d[c + 3]! - d[a + 3]!) * t;
      let y = d[a + 4]! + (sign * d[c + 4]! - d[a + 4]!) * t;
      let z = d[a + 5]! + (sign * d[c + 5]! - d[a + 5]!) * t;
      let w = d[a + 6]! + (sign * d[c + 6]! - d[a + 6]!) * t;
      const len = Math.hypot(x, y, z, w) || 1;
      x /= len;
      y /= len;
      z /= len;
      w /= len;
      const m = b * 12;
      mats[m] = 1 - 2 * (y * y + z * z);
      mats[m + 1] = 2 * (x * y - w * z);
      mats[m + 2] = 2 * (x * z + w * y);
      mats[m + 3] = 2 * (x * y + w * z);
      mats[m + 4] = 1 - 2 * (x * x + z * z);
      mats[m + 5] = 2 * (y * z - w * x);
      mats[m + 6] = 2 * (x * z - w * y);
      mats[m + 7] = 2 * (y * z + w * x);
      mats[m + 8] = 1 - 2 * (x * x + y * y);
      mats[m + 9] = d[a]! + (d[c]! - d[a]!) * t;
      mats[m + 10] = d[a + 1]! + (d[c + 1]! - d[a + 1]!) * t;
      mats[m + 11] = d[a + 2]! + (d[c + 2]! - d[a + 2]!) * t;
    }
    const local = model.local_positions;
    const bones = model.vert_bones;
    for (let i = 0; i < bones.length; i++) {
      const m = (bones[i]! % model.num_bones) * 12;
      const x = local[i * 3]!;
      const y = local[i * 3 + 1]!;
      const z = local[i * 3 + 2]!;
      out[i * 3] = mats[m]! * x + mats[m + 1]! * y + mats[m + 2]! * z + mats[m + 9]!;
      out[i * 3 + 1] = mats[m + 3]! * x + mats[m + 4]! * y + mats[m + 5]! * z + mats[m + 10]!;
      out[i * 3 + 2] = mats[m + 6]! * x + mats[m + 7]! * y + mats[m + 8]! * z + mats[m + 11]!;
    }
    viewer.setVertexPositions(out);
  }

  function render() {
    applyFrame(frame);
    if (!pose) return;
    const shown = Math.round(frame);
    frameInput.value = String(shown);
    frameLabel.textContent = `${shown + 1}/${pose.frames}`;
  }

  async function selectSequence(index: number) {
    const mine = ++token;
    note.textContent = t("common.loading");
    note.title = "";
    let loaded = cache.get(index);
    if (!loaded) {
      try {
        const raw = await invoke<MdlSeqFrames>("load_sequence", { path, index });
        loaded = { frames: raw.frames, bones: raw.bones, fps: raw.fps, looping: raw.looping, data: Float32Array.from(raw.data) };
        cache.set(index, loaded);
      } catch (err) {
        if (mine !== token) return;
        // sem quadros (ex.: animação em arquivo externo ausente): volta pra pose de repouso
        pose = null;
        setPlaying(false);
        setEnabled(false);
        frameLabel.textContent = "–";
        note.textContent = t("mdl.unavailable");
        note.title = String(err);
        viewer.setVertexPositions(model.positions);
        return;
      }
    }
    if (mine !== token) return;
    pose = loaded;
    frame = 0;
    frameInput.max = String(Math.max(0, pose.frames - 1));
    setEnabled(pose.frames > 1);
    setPlaying(pose.frames > 1);
    note.textContent = pose.looping ? t("mdl.loop") : t("mdl.once");
    render();
  }

  function tick(now: number) {
    raf = requestAnimationFrame(tick);
    const dt = Math.min((now - last) / 1000, 0.1);
    last = now;
    if (!playing || !pose || pose.frames < 2) return;
    // O último quadro de um ciclo repete o primeiro: o laço tem `frames - 1` passos (como o HLMV).
    const span = pose.frames - 1;
    frame += dt * pose.fps * speed;
    if (pose.looping) {
      frame %= span;
    } else if (frame >= span) {
      frame = span;
      setPlaying(false);
    }
    render();
  }

  seqSel.replaceChildren();
  model.sequence_info.forEach((info, i) => seqSel.append(new Option(info.name || t("mdl.seqN", i), String(i))));
  seqSel.addEventListener("change", () => void selectSequence(Number(seqSel.value)), on);

  playBtn.addEventListener(
    "click",
    () => {
      if (!pose) return;
      if (!playing && !pose.looping && frame >= pose.frames - 1) frame = 0; // recomeça o que já terminou
      setPlaying(!playing);
    },
    on,
  );
  frameInput.addEventListener(
    "input",
    () => {
      setPlaying(false);
      frame = Number(frameInput.value);
      render();
    },
    on,
  );
  speedSel.value = "1";
  speedSel.addEventListener("change", () => (speed = Number(speedSel.value) || 1), on);

  // Skin: só aparece se o modelo tem mais de uma família.
  skinSel.replaceChildren();
  model.skin_families.forEach((_, i) => skinSel.append(new Option(t("mdl.skinN", i + 1), String(i))));
  skinWrap.hidden = model.skin_families.length < 2;
  skinSel.addEventListener("change", () => viewer.setTextureMap(model.skin_families[Number(skinSel.value)] ?? []), on);

  setEnabled(false);
  setPlaying(false);
  frameLabel.textContent = "–";
  note.textContent = "";
  if (model.sequence_info.length > 0) void selectSequence(0);
  raf = requestAnimationFrame(tick);

  return {
    dispose() {
      ctl.abort();
      cancelAnimationFrame(raf);
      token++;
    },
  };
}
