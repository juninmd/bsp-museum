import { invoke } from "@tauri-apps/api/core";
import { t } from "../i18n.ts";
import {
  dataUrlToBytes,
  MAX_VOICES,
  soundColor,
  soundLabel,
  SpatialAudio,
  type AudioBackend,
  type SoundSource,
  type SoundState,
} from "../lib/spatial-audio.ts";
import type { AudioData, ModFileAnalysis } from "../types/modfiles.ts";
import type { AmbientSound } from "../types/bsp-extra.ts";
import type { Viewer3D } from "../viewer3d.ts";
import type { SoundVisual } from "../viewer-overlays.ts";

/**
 * Painel "Sons do mapa" da camada "Sons 3D": liga o motor de áudio espacial (`SpatialAudio`) ao
 * viewer. A câmera vira o ouvinte; a lista mostra, por `ambient_generic`, o arquivo, a distância,
 * o volume efetivo, o medidor de nível e os botões de mudo/solo/ouvir isolado. Clicar no nome leva
 * a câmera ao emissor.
 *
 * O contexto de áudio só nasce no clique em "Ativar áudio" (política de autoplay do navegador).
 */

export interface SoundPanel {
  /** mostra/esconde o painel; esconder pausa o áudio (sem soltar o contexto) */
  setOpen(on: boolean): void;
  /** para tudo, solta o contexto e tira o painel do DOM */
  dispose(): void;
}

interface Options {
  holder: HTMLElement;
  viewer: Viewer3D;
  /** pasta do mod (`<mod>/maps/x.bsp` -> `<mod>`) */
  modDir: string;
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

/** Lê os arquivos de som pelo backend Tauri (o `valve_root` sai do vizinho `valve` do mod). */
export function tauriAudioBackend(modRoot: string): AudioBackend {
  const args = (rel: string) => ({ modRoot, valveRoot: null, rel, origin: null });
  return {
    async analyze(rel) {
      const res = await invoke<ModFileAnalysis>("analyze_mod_file", args(rel));
      if (res.analysis.type !== "audio") throw new Error("não é um arquivo de áudio");
      return res.analysis;
    },
    async read(rel) {
      const data = await invoke<AudioData>("read_mod_audio", args(rel));
      return dataUrlToBytes(data.data_url);
    },
  };
}

/** `ambient_generic` do backend -> fonte do motor (o id é o índice na lista). */
export function toSources(list: readonly AmbientSound[]): SoundSource[] {
  return list.map((a, id) => ({
    id,
    entity: a.entity,
    targetname: a.targetname,
    file: a.sound,
    origin: a.origin,
    radius: a.radius,
    volume: a.volume,
    startSilent: a.start_silent,
  }));
}

const fmtBytes = (n: number): string => (n >= 1024 * 1024 ? `${(n / 1024 / 1024).toFixed(1)} MB` : `${Math.round(n / 1024)} KB`);

interface Row {
  src: SoundSource;
  li: HTMLLIElement;
  name: HTMLButtonElement;
  dist: HTMLElement;
  vol: HTMLElement;
  state: HTMLElement;
  fill: HTMLElement;
  meter: HTMLElement;
  mute: HTMLButtonElement;
  solo: HTMLButtonElement;
  listen: HTMLButtonElement;
  /** nível suavizado (queda lenta) */
  shown: number;
  last: string;
}

export function createSoundPanel(opts: Options): SoundPanel {
  const { holder, viewer, modDir } = opts;
  const sources = toSources(viewer.soundSources());
  const engine = new SpatialAudio(sources, tauriAudioBackend(modDir));
  let open = false;
  let disposed = false;
  let selected: number | null = null;
  let lastUi = 0;
  let lastSort = 0;
  let sortByDistance = false;

  // ---------------------------------------------------------------- estrutura
  const root = el("section", "snd-panel");
  root.hidden = true;
  root.setAttribute("aria-label", t("snd.title"));
  // cliques no painel não podem virar clique na cena (seleção de marcadores / órbita)
  for (const ev of ["pointerdown", "pointerup", "wheel", "dblclick"] as const) root.addEventListener(ev, (e) => e.stopPropagation());

  const head = el("div", "snd-head");
  const title = el("strong", undefined, t("snd.title"));
  const fold = el("button", "snd-fold", "–");
  fold.type = "button";
  fold.title = t("snd.collapse");
  fold.setAttribute("aria-label", t("snd.collapse"));
  fold.setAttribute("aria-expanded", "true");
  head.append(title, fold);

  const body = el("div", "snd-body");
  fold.addEventListener("click", () => {
    body.hidden = !body.hidden;
    fold.textContent = body.hidden ? "+" : "–";
    const label = body.hidden ? t("snd.expand") : t("snd.collapse");
    fold.title = label;
    fold.setAttribute("aria-label", label);
    fold.setAttribute("aria-expanded", String(!body.hidden));
  });

  // ativação (gesto do usuário)
  const enableBox = el("div", "snd-enable");
  const enableBtn = el("button", "snd-enable-btn", t("snd.enable"));
  enableBtn.type = "button";
  const enableHint = el("span", "dim small", t("snd.enableHint"));
  enableHint.setAttribute("aria-live", "polite");
  enableBox.append(enableBtn, enableHint);

  // controles (só depois de ativar)
  const controls = el("div", "snd-controls");
  controls.hidden = true;
  const masterLabel = el("label", "snd-master");
  const master = el("input");
  master.className = "snd-slider";
  master.type = "range";
  master.min = "0";
  master.max = "100";
  master.value = String(Math.round(engine.getMasterVolume() * 100));
  master.setAttribute("aria-label", t("snd.master"));
  masterLabel.append(el("span", "dim small", t("snd.master")), master);
  const muteLabel = el("label", "chk");
  const muteBox = el("input");
  muteBox.type = "checkbox";
  muteLabel.append(muteBox, document.createTextNode(` ${t("snd.muteAll")}`));
  const repeatLabel = el("label", "chk");
  const repeatBox = el("input");
  repeatBox.type = "checkbox";
  repeatBox.checked = engine.isRepeatAll();
  repeatLabel.title = t("snd.repeatHint");
  repeatLabel.append(repeatBox, document.createTextNode(` ${t("snd.repeat")}`));
  const sortLabel = el("label", "snd-sort");
  const sortSel = el("select");
  sortSel.append(new Option(t("snd.sort.map"), "map"), new Option(t("snd.sort.dist"), "dist"));
  sortLabel.append(el("span", undefined, `${t("snd.sort")}: `), sortSel);
  const masterRow = el("div", "snd-row-master");
  masterRow.append(masterLabel, muteLabel);
  controls.append(masterRow, repeatLabel);

  const summary = el("div", "dim small snd-summary");
  summary.setAttribute("aria-live", "off");
  summary.title = t("snd.summaryHint");
  const list = el("ul", "snd-list");
  list.setAttribute("role", "list");
  const sumRow = el("div", "snd-sumrow");
  sumRow.append(summary, sortLabel);
  const note = el("li", "dim small snd-note", t("snd.note"));
  body.append(enableBox, controls, sumRow, list);
  root.append(head, body);
  holder.append(root);

  // ---------------------------------------------------------------- linhas
  const rows: Row[] = sources.map((src) => {
    const li = el("li", "snd-row");
    li.dataset.id = String(src.id);
    const main = el("div", "snd-main");
    const dot = el("span", "snd-dot");
    dot.style.background = soundColor(src.id);
    const name = el("button", "snd-name", soundLabel(src.file));
    name.type = "button";
    name.title = `${src.file ?? "—"} · ${t("snd.focus")}`;
    name.addEventListener("click", () => {
      selected = src.id;
      viewer.focusOn([src.origin[0], src.origin[1], src.origin[2]]);
    });
    main.append(dot, name);

    const tags = el("div", "snd-tags dim small");
    tags.append(el("span", undefined, src.radius === null ? t("snd.everywhere") : t("snd.radius", Math.round(src.radius))));
    const loopTag = el("span", "snd-loop", t("snd.loop"));
    loopTag.hidden = true;
    tags.append(loopTag);
    if (src.startSilent) tags.append(el("span", undefined, t("snd.silent")));

    const meta = el("div", "snd-meta small");
    const dist = el("span", "snd-dist");
    const vol = el("span", "snd-vol");
    const state = el("span", "snd-state dim");
    meta.append(dist, vol, state);

    const meter = el("div", "snd-meter");
    meter.setAttribute("role", "meter");
    meter.setAttribute("aria-label", t("snd.level"));
    meter.setAttribute("aria-valuemin", "0");
    meter.setAttribute("aria-valuemax", "100");
    meter.setAttribute("aria-valuenow", "0");
    const fill = el("div", "snd-meter-fill");
    meter.append(fill);

    const btns = el("div", "snd-btns");
    const mute = el("button", "snd-btn", t("snd.mute"));
    mute.type = "button";
    mute.title = t("snd.muteTitle");
    mute.setAttribute("aria-pressed", "false");
    mute.addEventListener("click", () => {
      engine.setMuted(src.id, !engine.isMuted(src.id));
      refresh(true);
    });
    const solo = el("button", "snd-btn", t("snd.solo"));
    solo.type = "button";
    solo.title = t("snd.soloTitle");
    solo.setAttribute("aria-pressed", "false");
    solo.addEventListener("click", () => {
      engine.setSolo(src.id, !engine.isSolo(src.id));
      refresh(true);
    });
    const listen = el("button", "snd-btn snd-listen", t("snd.listen"));
    listen.type = "button";
    listen.title = t("snd.listenTitle");
    listen.setAttribute("aria-pressed", "false");
    listen.addEventListener("click", () => {
      selected = src.id;
      void withAudio(() => {
        engine.setPreview(engine.getPreview() === src.id ? null : src.id);
        refresh(true);
      });
    });
    btns.append(mute, solo, listen);

    li.append(main, btns, tags, meta, meter);
    list.append(li);
    return { src, li, name, dist, vol, state, fill, meter, mute, solo, listen, shown: 0, last: "" };
  });

  if (!rows.length) list.append(el("li", "dim small", t("snd.empty")));
  list.append(note);

  // ---------------------------------------------------------------- ativação
  let starting = false;
  const activate = async () => {
    if (starting || engine.enabled) return;
    starting = true;
    enableBtn.disabled = true;
    enableHint.textContent = t("snd.starting");
    try {
      await engine.enable();
      if (disposed) return;
      enableBox.hidden = true;
      controls.hidden = false;
    } catch (err) {
      enableBtn.disabled = false;
      enableHint.textContent = t("snd.error", err instanceof Error ? err.message : String(err));
    } finally {
      starting = false;
    }
    refresh(true);
  };
  enableBtn.addEventListener("click", () => void activate());

  /** roda `fn` depois de garantir o áudio ativo (o clique do usuário já é o gesto) */
  const withAudio = async (fn: () => void) => {
    if (!engine.enabled) await activate();
    if (engine.enabled) fn();
  };

  master.addEventListener("input", () => engine.setMasterVolume(Number(master.value) / 100));
  muteBox.addEventListener("change", () => {
    engine.setMuteAll(muteBox.checked);
    refresh(true);
  });
  repeatBox.addEventListener("change", () => engine.setRepeatAll(repeatBox.checked));
  sortSel.addEventListener("change", () => {
    sortByDistance = sortSel.value === "dist";
    lastSort = 0;
    refresh(true);
  });

  // ---------------------------------------------------------------- atualização
  const stateText = (s: SoundState): string => {
    if (s.reason) return t(`snd.un.${s.reason.code}`, s.reason.detail ?? "");
    if (s.muted && !s.solo) return t("snd.st.muted");
    switch (s.status) {
      case "playing":
        return t("snd.st.playing");
      case "loading":
        return t("snd.st.loading");
      case "done":
        return t("snd.st.done");
      default:
        return s.inRange ? t("snd.st.near") : t("snd.st.idle");
    }
  };

  const visuals: (SoundVisual | undefined)[] = new Array(sources.length);

  function refresh(force = false): void {
    if (disposed) return;
    const now = performance.now();
    if (!force && now - lastUi < 90) return;
    lastUi = now;
    const states = engine.states();
    let playing = 0;
    for (const s of states) {
      const row = rows[s.id];
      if (!row) continue;
      if (s.status === "playing") playing++;
      // decay do medidor: sobe na hora, cai devagar
      row.shown = Math.max(s.level, row.shown * 0.82);
      const pct = Math.round(row.shown * 100);
      const key = [s.status, s.reason?.code, s.muted, s.solo, s.previewing, s.loops, Math.round(s.distance), Math.round(s.gain * 100), pct >> 2, selected === s.id].join("|");
      visuals[s.id] = {
        playing: s.status === "playing",
        level: row.shown,
        unavailable: s.status === "unavailable",
        silenced: s.silenced,
        selected: selected === s.id,
      };
      if (key === row.last) continue;
      row.last = key;
      row.li.dataset.status = s.status;
      row.li.classList.toggle("is-unavailable", s.status === "unavailable");
      row.li.classList.toggle("is-playing", s.status === "playing");
      row.li.classList.toggle("is-selected", selected === s.id);
      row.dist.textContent = t("snd.dist", Math.round(s.distance).toLocaleString(t("locale")));
      row.vol.textContent = t("snd.vol", Math.round(s.gain * 100));
      row.state.textContent = stateText(s);
      row.fill.style.width = `${pct}%`;
      row.meter.setAttribute("aria-valuenow", String(pct));
      row.meter.hidden = s.status !== "playing";
      row.mute.setAttribute("aria-pressed", String(s.muted));
      row.solo.setAttribute("aria-pressed", String(s.solo));
      row.listen.setAttribute("aria-pressed", String(s.previewing));
      row.listen.textContent = s.previewing ? t("snd.listenStop") : t("snd.listen");
      row.listen.disabled = s.status === "unavailable";
      const loopTag = row.li.querySelector<HTMLElement>(".snd-loop");
      if (loopTag) loopTag.hidden = !s.loops;
    }
    viewer.setSoundVisuals(visuals);
    summary.textContent = t("snd.summary", playing, MAX_VOICES, fmtBytes(engine.cacheBytes()));
    if (sortByDistance && (force || now - lastSort > 1000)) {
      lastSort = now;
      const order = [...states].sort((a, b) => a.distance - b.distance || a.id - b.id);
      let prev: HTMLElement | null = null;
      for (const s of order) {
        const li = rows[s.id]!.li;
        if (li.previousElementSibling !== prev) (prev ? prev.after(li) : list.prepend(li));
        prev = li;
      }
      list.append(note);
    } else if (!sortByDistance && force && sortSel.value === "map") {
      for (const r of rows) list.append(r.li);
      list.append(note);
    }
  }
  engine.onChange(() => refresh(true));

  // a câmera é o ouvinte: pose a cada quadro
  viewer.onFrame((pose, nowMs) => {
    if (!open) return;
    engine.update(pose.pos, pose.forward, pose.up, nowMs);
    refresh();
  });

  return {
    setOpen(on) {
      if (disposed || on === open) return;
      open = on;
      root.hidden = !on;
      if (on) {
        engine.probe();
        if (engine.enabled) void engine.enable(); // volta de uma pausa (o clique na caixa é o gesto)
        refresh(true);
      } else {
        void engine.pause();
        selected = null;
        viewer.setSoundVisuals([]);
      }
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      viewer.onFrame(null);
      engine.dispose();
      root.remove();
    },
  };
}
