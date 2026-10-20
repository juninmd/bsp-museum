import { invoke } from "@tauri-apps/api/core";
import { t } from "./i18n.ts";
import {
  BOX_STRIDE,
  BoundedCache,
  blendAxes,
  blendValue,
  bodyChoices,
  controllerSliderMax,
  controllersArg,
  debounce,
  defaultBlendByte,
  formatControllerValue,
  framePercent,
  hitGroupColor,
  hueColor,
  lerpBoxes,
  restControllerBytes,
  seqCacheKey,
  sliderToControllerByte,
  type BlendAxis,
} from "./lib/mdl-controls.ts";
import { mountModelViewer3D, type ModelViewer } from "./model-viewer.ts";
import { bodyValue, type MdlSeqFramesEx, type MdlSummaryEx } from "./types/mdl-extra.ts";
import { mountSkins, type Skins } from "./ui/skins.ts";

/** floats por bone por quadro: posição (3) + quaternion xyzw (4) — mesmo layout de `mdl::POSE_STRIDE` */
const STRIDE = 7;

/** quadros em memória por modelo aberto (cada combinação sequência/blend/controladores é uma entrada) */
const POSE_CACHE_MAX = 48;

/** espera após o último movimento de um slider antes de pedir os quadros de novo */
const SLIDER_DEBOUNCE_MS = 120;

interface Pose {
  frames: number;
  bones: number;
  fps: number;
  looping: boolean;
  data: Float32Array;
  /** origem de cada attachment por quadro (frames * attachments * 3) */
  attachments: Float32Array | null;
  /** caixas por quadro (frames * hitboxes * 10); só vem se o overlay estava ligado */
  hitboxes: Float32Array | null;
}

/**
 * Visualizador de um `.mdl` isolado: cena orbitável própria (`model-viewer.ts`)
 * + a barra de ferramentas (sequência, tocar/pausar, quadro, velocidade, skin)
 * e o painel de controles (bodygroups, cor de time, controladores, blends,
 * overlays, eventos), ligados por `bindAnimation`; `path` é o arquivo, pra pedir
 * os quadros e recarregar o modelo.
 */
export function mountModelViewer(container: HTMLElement, model: MdlSummaryEx, path: string): { dispose(): void } {
  const viewer = mountModelViewer3D(container, model);
  const animation = bindAnimation(viewer, model, path);
  return {
    dispose() {
      animation.dispose();
      viewer.dispose();
    },
  };
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

/**
 * Animação por skinning na CPU: pra cada quadro o backend entrega a pose de
 * mundo (posição + quaternion) de cada bone; aqui interpolamos entre dois
 * quadros, montamos a matriz de cada bone e movemos os vértices
 * (`local_positions`, no espaço do bone dono) — mesma conta do SDK
 * (`pose[bone] * local`). Os quadros de cada sequência vêm sob demanda
 * (`load_sequence`, com blends/controladores/hitboxes) e ficam em cache
 * enquanto o modelo está aberto.
 */
function bindAnimation(viewer: ModelViewer, initial: MdlSummaryEx, path: string): { dispose(): void } {
  const byId = <T extends HTMLElement>(id: string) => document.querySelector<T>(`#${id}`)!;
  const seqSel = byId<HTMLSelectElement>("res-sequence");
  const playBtn = byId<HTMLButtonElement>("res-play");
  const frameInput = byId<HTMLInputElement>("res-frame");
  const frameLabel = byId<HTMLElement>("res-frame-label");
  const ticks = byId<HTMLElement>("res-frame-ticks");
  const speedSel = byId<HTMLSelectElement>("res-speed");
  const skinWrap = byId<HTMLElement>("res-skin-wrap");
  const skinSel = byId<HTMLSelectElement>("res-skin");
  const note = byId<HTMLElement>("res-anim-note");
  const panel = byId<HTMLElement>("res-panel");

  const ctl = new AbortController();
  const on = { signal: ctl.signal };
  const cache = new BoundedCache<Pose>(POSE_CACHE_MAX);

  let model = initial;
  let out = new Float32Array(model.local_positions.length);
  let mats = new Float32Array(model.num_bones * 12); // por bone: 3x3 (linha a linha) + posição
  let pose: Pose | null = null;
  let frame = 0;
  let playing = false;
  let speed = 1;
  let seq = 0;
  let needReset = false; // troca de sequência ainda não aplicada (um slider mexido no meio não a cancela)
  let token = 0; // descarta resposta de uma sequência que já não é a escolhida
  let reloadToken = 0; // idem para o recarregamento do modelo (bodygroup, cor de time)
  let raf = 0;
  let last = performance.now();

  // ---- estado dos controles
  const parts = initial.bodyparts ?? [];
  const controllers = [...(initial.controllers ?? [])].sort((a, b) => a.index - b.index);
  const hitboxInfo = initial.hitboxes ?? [];
  const attachInfo = initial.attachments ?? [];
  const hasRemap = (initial.texture_info ?? []).some((i) => i.remap);
  const choices = bodyChoices(parts, initial.selection?.body ?? 0);
  let top: number | null = initial.selection?.top ?? null;
  let bottom: number | null = initial.selection?.bottom ?? null;
  const ctlBytes = restControllerBytes(controllers);
  const blend: [number, number] = [0, 0];
  let showHit = false;
  let showAtt = false;
  let skinMap: number[] = [];
  let activeSkin = 0;
  let skins: Skins | null = null;

  const hitGroups = hitboxInfo.map((h) => h.group);
  const restBoxes = new Float32Array(hitboxInfo.length * BOX_STRIDE);
  hitboxInfo.forEach((h, i) => restBoxes.set([...h.rest.center, ...h.rest.half, ...h.rest.rot], i * BOX_STRIDE));
  const restPoints = new Float32Array(attachInfo.flatMap((a) => a.rest));
  const boxBuf = new Float32Array(hitboxInfo.length * BOX_STRIDE);
  const pointBuf = new Float32Array(attachInfo.length * 3);

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

  /** hitboxes e attachments do quadro `f` (ou do repouso, quando não há pose) */
  function applyOverlays(f: number) {
    if (showHit && hitboxInfo.length > 0) {
      if (!pose) viewer.setHitboxes(restBoxes, hitGroups);
      else if (pose.hitboxes) {
        lerpBoxes(pose.hitboxes, pose.frames, hitboxInfo.length, f, boxBuf);
        viewer.setHitboxes(boxBuf, hitGroups);
      } else viewer.setHitboxes(null); // as caixas deste quadro ainda estão a caminho
    } else viewer.setHitboxes(null);
    if (showAtt && attachInfo.length > 0) {
      if (pose?.attachments) {
        lerpBoxes(pose.attachments, pose.frames, attachInfo.length, f, pointBuf, 3);
        viewer.setAttachments(pointBuf, attachInfo.map((a) => a.name));
      } else viewer.setAttachments(restPoints, attachInfo.map((a) => a.name));
    } else viewer.setAttachments(null);
  }

  function render() {
    applyFrame(frame);
    applyOverlays(frame);
    if (!pose) return;
    const shown = Math.round(frame);
    frameInput.value = String(shown);
    frameLabel.textContent = `${shown + 1}/${pose.frames}`;
    markCurrentEvents(shown);
  }

  // ------------------------------------------------------------------ quadros da sequência

  /** parâmetros de `load_sequence` para o estado atual dos controles */
  function request(index: number) {
    const blends = model.sequence_info[index]?.blends ?? 1;
    const blendArg: [number, number] | undefined = blends > 1 ? [blend[0], blend[1]] : undefined;
    const controllersParam = controllersArg(controllers, ctlBytes);
    const hitboxes = showHit && hitboxInfo.length > 0;
    return { blendArg, controllersParam, hitboxes };
  }

  async function fetchPose(index: number): Promise<Pose> {
    const { blendArg, controllersParam, hitboxes } = request(index);
    const key = seqCacheKey(index, blendArg, controllersParam, hitboxes);
    // um conjunto com hitboxes serve também a quem não pede
    const hit = cache.get(key) ?? (hitboxes ? undefined : cache.get(seqCacheKey(index, blendArg, controllersParam, true)));
    if (hit) return hit;
    const args: Record<string, unknown> = { path, index };
    if (blendArg) args.blend = blendArg;
    if (controllersParam) args.controllers = controllersParam;
    if (hitboxes) args.hitboxes = true;
    const raw = await invoke<MdlSeqFramesEx>("load_sequence", args);
    const sized = (v: number[] | undefined, per: number) => (v && v.length === raw.frames * per ? Float32Array.from(v) : null);
    const loaded: Pose = {
      frames: raw.frames,
      bones: raw.bones,
      fps: raw.fps,
      looping: raw.looping,
      data: Float32Array.from(raw.data),
      attachments: sized(raw.attachments, attachInfo.length * 3),
      hitboxes: sized(raw.hitboxes, hitboxInfo.length * BOX_STRIDE),
    };
    cache.set(key, loaded);
    return loaded;
  }

  /**
   * Mostra a sequência `index`. `reset` = voltou ao quadro 0 e toca (troca de sequência);
   * sem ele (mexeu num slider) o quadro e o estado de tocar/pausar continuam, e uma falha
   * mantém a pose que já estava na tela.
   */
  async function showSequence(index: number, reset: boolean) {
    const mine = ++token;
    if (reset) needReset = true;
    reset = needReset;
    if (reset) {
      note.textContent = t("common.loading");
      note.title = "";
    }
    let loaded: Pose;
    try {
      loaded = await fetchPose(index);
    } catch (err) {
      if (mine !== token) return;
      note.title = String(err);
      if (!reset && pose) {
        note.textContent = t("mdl.unavailable");
        return;
      }
      // sem quadros (ex.: animação em arquivo externo ausente): volta pra pose de repouso
      pose = null;
      setPlaying(false);
      setEnabled(false);
      frameLabel.textContent = "–";
      note.textContent = t("mdl.unavailable");
      viewer.setVertexPositions(model.positions);
      applyOverlays(0);
      return;
    }
    if (mine !== token) return;
    pose = loaded;
    needReset = false;
    frameInput.max = String(Math.max(0, pose.frames - 1));
    setEnabled(pose.frames > 1);
    if (reset) {
      frame = 0;
      setPlaying(pose.frames > 1);
    } else {
      frame = Math.min(frame, Math.max(0, pose.frames - 1));
      if (pose.frames < 2) setPlaying(false);
    }
    note.textContent = pose.looping ? t("mdl.loop") : t("mdl.once");
    note.title = "";
    render();
  }

  const refreshPose = debounce(() => void showSequence(seq, false), SLIDER_DEBOUNCE_MS);

  // ------------------------------------------------------------------ recarregar o modelo (bodygroup, cor de time)

  async function reloadModel() {
    const mine = ++reloadToken;
    const args: Record<string, unknown> = { path, body: bodyValue(parts, choices) };
    if (top !== null) args.top = top;
    if (bottom !== null) args.bottom = bottom;
    try {
      const next = await invoke<MdlSummaryEx>("load_model", args);
      if (mine !== reloadToken) return;
      await viewer.setModel(next);
      if (mine !== reloadToken) return;
      model = next;
      out = new Float32Array(model.local_positions.length);
      mats = new Float32Array(model.num_bones * 12);
      viewer.setTextureMap(skinMap);
      skins?.setModel(next);
      if (pose) render();
      else {
        viewer.setVertexPositions(model.positions);
        applyOverlays(0);
      }
    } catch (err) {
      if (mine !== reloadToken) return;
      note.textContent = t("mdl.reloadFailed");
      note.title = String(err);
    }
  }
  const scheduleReload = debounce(() => void reloadModel(), SLIDER_DEBOUNCE_MS);

  // ------------------------------------------------------------------ painel

  const blendBox = el("div", "mdl-blend");
  const eventBox = el("div", "mdl-events");
  let eventRows: { frame: number; row: HTMLElement }[] = [];

  function section(title: string, hint?: string): { root: HTMLElement; body: HTMLElement } {
    const root = el("section", "mdl-sec");
    const h = el("h4", undefined, title);
    if (hint) h.title = hint;
    const body = el("div", "mdl-sec-body");
    root.append(h, body);
    return { root, body };
  }

  /** linha "rótulo | slider | valor"; devolve o input e o setter do texto do valor */
  function sliderRow(label: string, max: number, value: number, title?: string) {
    const row = el("label", "mdl-row");
    if (title) row.title = title;
    const name = el("span", "mdl-name", label);
    const input = el("input");
    input.type = "range";
    input.min = "0";
    input.max = String(max);
    input.step = "1";
    input.value = String(value);
    const val = el("span", "mdl-val");
    row.append(name, input, val);
    return { row, input, val };
  }

  function buildBodygroups(): HTMLElement | null {
    const multi = parts.map((p, i) => ({ p, i })).filter(({ p }) => p.models.length >= 2);
    if (multi.length === 0) return null;
    const { root, body } = section(t("mdl.bodygroups"));
    for (const { p, i } of multi) {
      const row = el("label", "mdl-row");
      row.append(el("span", "mdl-name", p.name || t("mdl.bodypartN", i + 1)));
      const sel = el("select");
      p.models.forEach((name, k) => sel.append(new Option(name || `#${k}`, String(k))));
      sel.value = String(choices[i] ?? 0);
      sel.addEventListener(
        "change",
        () => {
          choices[i] = Number(sel.value);
          scheduleReload();
        },
        on,
      );
      row.append(sel);
      body.append(row);
    }
    return root;
  }

  function buildTeamColors(): HTMLElement | null {
    if (!hasRemap) return null;
    const { root, body } = section(t("mdl.teamColors"), t("mdl.teamColorsHint"));
    const make = (label: string, get: () => number | null, set: (v: number | null) => void) => {
      const { row, input, val } = sliderRow(label, 255, get() ?? 0);
      // a amostra mostra o matiz; clicar nela volta à cor do modelo
      const swatch = el("button", "mdl-swatch");
      swatch.type = "button";
      swatch.title = t("mdl.default");
      swatch.setAttribute("aria-label", t("mdl.default"));
      row.querySelector(".mdl-name")!.prepend(swatch);
      const show = () => {
        const v = get();
        swatch.style.background = v === null ? "transparent" : hueColor(v);
        swatch.classList.toggle("off", v === null);
        val.textContent = v === null ? t("mdl.default") : String(v);
      };
      input.addEventListener(
        "input",
        () => {
          set(Number(input.value));
          show();
          scheduleReload();
        },
        on,
      );
      swatch.addEventListener(
        "click",
        () => {
          if (get() === null) return;
          set(null);
          show();
          scheduleReload();
        },
        on,
      );
      show();
      body.append(row);
    };
    make(t("mdl.teamTop"), () => top, (v) => (top = v));
    make(t("mdl.teamBottom"), () => bottom, (v) => (bottom = v));
    return root;
  }

  function buildControllers(): HTMLElement | null {
    if (controllers.length === 0) return null;
    const { root, body } = section(t("mdl.controllers"), t("mdl.controllersHint"));
    const rows: { c: (typeof controllers)[number]; input: HTMLInputElement; val: HTMLElement }[] = [];
    for (const c of controllers) {
      const label = c.index > 3 ? t("mdl.mouth") : t("mdl.controllerN", c.index + 1);
      const range = `${c.kind}: ${c.start} … ${c.end}${c.unit === "deg" ? "°" : ""}${c.wrap ? " (360°)" : ""}`;
      const { row, input, val } = sliderRow(label, controllerSliderMax(c), ctlBytes[c.index] ?? 0, range);
      const show = () => (val.textContent = formatControllerValue(c, ctlBytes[c.index] ?? 0));
      input.addEventListener(
        "input",
        () => {
          ctlBytes[c.index] = sliderToControllerByte(c, Number(input.value));
          show();
          refreshPose();
        },
        on,
      );
      show();
      rows.push({ c, input, val });
      body.append(row);
    }
    const reset = el("button", "mdl-mini", t("mdl.resetControllers"));
    reset.type = "button";
    reset.addEventListener(
      "click",
      () => {
        const rest = restControllerBytes(controllers);
        for (const r of rows) {
          ctlBytes[r.c.index] = rest[r.c.index] ?? 0;
          r.input.value = String(ctlBytes[r.c.index]);
          r.val.textContent = formatControllerValue(r.c, ctlBytes[r.c.index] ?? 0);
        }
        refreshPose();
      },
      on,
    );
    body.append(reset);
    return root;
  }

  /** sliders de blend da sequência atual (1 ou 2 eixos) */
  function buildBlend() {
    blendBox.replaceChildren();
    const axes = blendAxes(model.sequence_info[seq]?.blends ?? 1, model.seq_details?.[seq]);
    blendBox.hidden = axes.length === 0;
    if (axes.length === 0) return;
    const { root, body } = section(t("mdl.blends"), t("mdl.blendsHint", model.sequence_info[seq]?.blends ?? 1));
    for (const a of axes) {
      const label = axes.length > 1 ? t("mdl.blendN", a.which + 1) : t("mdl.blend");
      const { row, input, val } = sliderRow(axisTitle(label, a), 255, blend[a.which]);
      const show = () => (val.textContent = blendLabel(a, blend[a.which]));
      input.addEventListener(
        "input",
        () => {
          blend[a.which] = Number(input.value);
          show();
          refreshPose();
        },
        on,
      );
      show();
      body.append(row);
    }
    blendBox.append(root);
  }

  function axisTitle(label: string, a: BlendAxis): string {
    return a.axis ? `${label} (${a.axis})` : label;
  }

  function blendLabel(a: BlendAxis, byte: number): string {
    if (!a.axis) return String(byte);
    const v = Math.round(blendValue(a, byte));
    return a.axis.endsWith("r") ? `${v}°` : String(v);
  }

  /** blends começam no "zero" da faixa (mira reta) quando ela o cruza */
  function resetBlends() {
    const d = model.seq_details?.[seq];
    const axes = blendAxes(model.sequence_info[seq]?.blends ?? 1, d);
    blend[0] = 0;
    blend[1] = 0;
    for (const a of axes) blend[a.which] = defaultBlendByte(d, a);
  }

  function buildOverlays(): HTMLElement | null {
    if (hitboxInfo.length === 0 && attachInfo.length === 0) return null;
    const { root, body } = section(t("mdl.overlays"));
    const check = (label: string, get: () => boolean, set: (v: boolean) => void) => {
      const row = el("label", "mdl-check");
      const box = el("input");
      box.type = "checkbox";
      box.checked = get();
      box.addEventListener(
        "change",
        () => {
          set(box.checked);
          if (showHit && pose && !pose.hitboxes) void showSequence(seq, false); // pede as caixas por quadro
          else applyOverlays(frame);
        },
        on,
      );
      row.append(box, el("span", undefined, label));
      body.append(row);
      return row;
    };
    if (hitboxInfo.length > 0) {
      check(t("mdl.hitboxes", hitboxInfo.length), () => showHit, (v) => (showHit = v));
      const legend = el("div", "mdl-legend");
      for (const g of [...new Set(hitGroups)].sort((a, b) => a - b)) {
        const chip = el("span", "mdl-chip", String(g));
        chip.style.borderColor = `#${hitGroupColor(g).toString(16).padStart(6, "0")}`;
        chip.title = hitGroupName(g);
        legend.append(chip);
      }
      body.append(legend);
    }
    if (attachInfo.length > 0) {
      check(t("mdl.attachments", attachInfo.length), () => showAtt, (v) => (showAtt = v));
    }
    return root;
  }

  function hitGroupName(g: number): string {
    return g >= 0 && g <= 7 ? t(`mdl.hitgroup${g}`) : t("mdl.hitgroupN", g);
  }

  /** eventos da sequência atual: lista clicável + marcadores na barra do tempo */
  function buildEvents() {
    eventBox.replaceChildren();
    ticks.replaceChildren();
    eventRows = [];
    const events = model.seq_details?.[seq]?.events ?? [];
    eventBox.hidden = events.length === 0;
    if (events.length === 0) return;
    const frames = model.sequence_info[seq]?.frames ?? 0;
    const { root, body } = section(t("mdl.events", events.length));
    const list = el("ul", "mdl-ev-list");
    for (const e of events) {
      const li = el("li");
      const btn = el("button", "mdl-ev");
      btn.type = "button";
      const name = e.name ?? `#${e.event}`;
      btn.title = `${t("mdl.eventId", e.event)} · ${t(e.side === "client" ? "mdl.eventClient" : "mdl.eventServer")}`;
      btn.append(el("span", "mdl-ev-frame", t("mdl.eventFrame", e.frame)), el("span", "mdl-ev-name", name));
      if (e.options) btn.append(el("span", "mdl-ev-opt", e.options));
      btn.addEventListener("click", () => gotoFrame(e.frame), on);
      li.append(btn);
      list.append(li);
      eventRows.push({ frame: e.frame, row: btn });
      const tick = el("span", "frame-tick");
      tick.title = `${t("mdl.eventFrame", e.frame)} · ${name}`;
      const pct = framePercent(e.frame, frames) / 100;
      tick.style.left = `calc(${pct} * (100% - 14px) + 7px)`;
      ticks.append(tick);
    }
    body.append(list);
    eventBox.append(root);
  }

  function gotoFrame(f: number) {
    if (!pose) return;
    setPlaying(false);
    frame = Math.max(0, Math.min(f, pose.frames - 1));
    render();
  }

  function markCurrentEvents(shown: number) {
    for (const r of eventRows) r.row.classList.toggle("now", r.frame === shown);
  }

  function buildWarnings(): HTMLElement | null {
    const warnings = model.warnings ?? [];
    if (warnings.length === 0 && !model.external_textures) return null;
    const root = el("div", "mdl-notes");
    if (model.external_textures) root.append(el("p", "mdl-note", t("mdl.externalTextures")));
    if (warnings.length > 0) {
      const det = el("details", "mdl-warn");
      det.append(el("summary", undefined, t("mdl.warnings", warnings.length)));
      const ul = el("ul");
      for (const w of warnings) ul.append(el("li", undefined, w));
      det.append(ul);
      root.append(det);
    }
    return root;
  }

  function buildPanel() {
    const blocks = [buildBodygroups(), buildTeamColors(), buildControllers(), buildOverlays()];
    buildBlend();
    buildEvents();
    const notes = buildWarnings();
    panel.replaceChildren(...blocks.filter((b): b is HTMLElement => b !== null), blendBox, eventBox, ...(notes ? [notes] : []));
    panel.hidden = panel.children.length === 0 || [...panel.children].every((c) => (c as HTMLElement).hidden);
  }

  // ------------------------------------------------------------------ barra de animação

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

  function selectSequence(index: number) {
    seq = index;
    resetBlends();
    buildBlend();
    buildEvents();
    panel.hidden = [...panel.children].every((c) => (c as HTMLElement).hidden);
    void showSequence(index, true);
  }

  seqSel.replaceChildren();
  model.sequence_info.forEach((info, i) => seqSel.append(new Option(info.name || t("mdl.seqN", i), String(i))));
  seqSel.addEventListener("change", () => selectSequence(Number(seqSel.value)), on);

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

  /** aplica uma família de skin: seletor da barra, texturas do visualizador e a faixa de skins */
  function setSkin(index: number) {
    activeSkin = index;
    skinMap = model.skin_families[index] ?? [];
    skinSel.value = String(index);
    viewer.setTextureMap(skinMap);
    skins?.setSkin(index);
  }
  skinSel.addEventListener("change", () => setSkin(Number(skinSel.value)), on);

  // Faixa de skins (galeria 3D, comparação, equipes, mapa UV): fica abaixo do visualizador.
  const anchor = skinWrap.closest<HTMLElement>(".res-viewer-wrap");
  if (anchor) {
    skins = mountSkins({
      viewer,
      anchor,
      model: () => model,
      skin: () => activeSkin,
      setSkin,
      loadVariant: (topColor, bottomColor) =>
        invoke<MdlSummaryEx>("load_model", { path, body: bodyValue(parts, choices), top: topColor, bottom: bottomColor }),
    });
  }

  // Trocar o idioma refaz os textos do painel (o estado dos controles fica).
  const langWatch = new MutationObserver(() => {
    buildPanel();
    setPlaying(playing);
    if (pose) note.textContent = pose.looping ? t("mdl.loop") : t("mdl.once");
  });
  langWatch.observe(document.documentElement, { attributes: true, attributeFilter: ["lang"] });

  setEnabled(false);
  setPlaying(false);
  frameLabel.textContent = "–";
  note.textContent = "";
  resetBlends();
  buildPanel();
  if (model.sequence_info.length > 0) void showSequence(0, true);
  raf = requestAnimationFrame(tick);

  return {
    dispose() {
      ctl.abort();
      skins?.dispose();
      skins = null;
      langWatch.disconnect();
      refreshPose.cancel();
      scheduleReload.cancel();
      cancelAnimationFrame(raf);
      token++;
      reloadToken++;
      panel.replaceChildren();
      panel.hidden = true;
      ticks.replaceChildren();
    },
  };
}
