/**
 * Áudio espacial da vista 3D: os `ambient_generic` do mapa viram fontes sonoras de um
 * `AudioContext` (Web Audio API), com o `AudioListener` preso à câmera.
 *
 * Duas partes:
 * - funções puras (atenuação por distância, escolha dos N sons mais próximos, mapeamento de
 *   volume, região de loop, cache por memória…), testáveis sem navegador (`spatial-audio.test.ts`);
 * - `SpatialAudio`, o motor que as usa com o Web Audio. Nada aqui toca o Tauri: quem monta o motor
 *   entrega um `AudioBackend` (no app, `analyze_mod_file` + `read_mod_audio`).
 *
 * Coordenadas: as posições chegam em GoldSrc (Z-up) e entram na cena Three.js (Y-up) por
 * `toWorld` — a mesma troca `(x, y, z) -> (x, z, -y)` que o resto do viewer usa. A unidade é a
 * mesma (1 unidade do mapa = 1 unidade da cena), então o raio do backend vale direto como
 * `maxDistance` do `PannerNode`.
 *
 * Atenuação: o GoldSrc é linear (`ganho = 1 - distância * atenuação / 1000`, zero no raio
 * "nominal"), então o panner usa `distanceModel = "linear"` e `maxDistance = raio`, que cai a
 * zero justamente no limite audível. Um pequeno platô (`refDistance`) evita o salto de volume
 * quando a câmera atravessa o emissor. Escrito de memória do motor, não conferido contra o
 * GoldSrc original.
 */

import { toWorld } from "./geometry.ts";
import type { AudioInfo, LoopPoint } from "../types/modfiles.ts";

export type Vec3 = readonly [number, number, number];

// ------------------------------------------------------------------ limites

/** sons tocando ao mesmo tempo (os mais próximos) */
export const MAX_VOICES = 16;
/** memória de áudio decodificado mantida em cache (bytes de PCM float32) */
export const MAX_CACHE_BYTES = 64 * 1024 * 1024;
/** cargas simultâneas ao backend */
export const MAX_LOADS = 2;
/** quantos arquivos diferentes a sondagem em segundo plano inspeciona */
export const MAX_PROBES = 400;
/** intervalo entre reavaliações dos sons ativos (ms) */
export const SELECT_INTERVAL_MS = 100;

// ------------------------------------------------------------------ fontes

/** Um `ambient_generic` do mapa, como o motor o enxerga. */
export interface SoundSource {
  /** índice estável na lista de sons do viewer */
  id: number;
  /** posição da entidade na lista de entidades do mapa */
  entity: number;
  targetname: string | null;
  /** `message` cru (nome do arquivo, a partir de `sound/`); `null` = vazio */
  file: string | null;
  /** posição GoldSrc (Z-up) */
  origin: Vec3;
  /** unidades; `null` = toca em todo o mapa */
  radius: number | null;
  /** 0..1 */
  volume: number;
  /** começa silencioso (flag 16): só toca se o usuário pedir */
  startSilent: boolean;
}

/** Posição da fonte na cena Three.js (Y-up). */
export function sourcePosition(s: Pick<SoundSource, "origin">): [number, number, number] {
  return toWorld(s.origin[0], s.origin[1], s.origin[2]);
}

// ------------------------------------------------------------------ arquivo

export type SoundFile =
  | { ok: true; rel: string }
  | { ok: false; reason: "noFile" | "sentence" };

/**
 * Caminho do arquivo dentro do mod. O `message` é relativo a `sound/`; nomes com `!` ou `#`
 * na frente são sentenças do `sentences.txt` (vozes do jogo), que não são um arquivo.
 */
export function soundRel(message: string | null): SoundFile {
  const raw = (message ?? "").trim();
  if (!raw) return { ok: false, reason: "noFile" };
  if (raw.startsWith("!") || raw.startsWith("#")) return { ok: false, reason: "sentence" };
  const clean = raw.replace(/\\/g, "/").replace(/^\/+/, "");
  const rel = /^sound\//i.test(clean) ? clean : `sound/${clean}`;
  return { ok: true, rel };
}

/** Nome curto para a lista (`ambience/wind.wav` -> `wind.wav`). */
export function soundLabel(message: string | null): string {
  const raw = (message ?? "").trim().replace(/\\/g, "/");
  if (!raw) return "—";
  return raw.slice(raw.lastIndexOf("/") + 1);
}

// ------------------------------------------------------------------ distância e volume

/** Parâmetros do `PannerNode` para um raio audível (`null` = sem espacialização). */
export interface PannerParams {
  refDistance: number;
  maxDistance: number;
  rolloffFactor: number;
}

/** Fração do raio em que o volume ainda é o cheio (platô ao redor do emissor). */
export const REF_FRACTION = 0.06;

export function pannerParams(radius: number): PannerParams {
  const max = Math.max(2, radius);
  return { refDistance: Math.max(1, max * REF_FRACTION), maxDistance: max, rolloffFactor: 1 };
}

/**
 * Ganho do `distanceModel = "linear"` do Web Audio:
 * `1 - rolloff * (clamp(d, ref, max) - ref) / (max - ref)`. Zero em `d >= max`.
 */
export function distanceGain(distance: number, p: PannerParams): number {
  const d = Math.min(Math.max(distance, p.refDistance), p.maxDistance);
  const span = p.maxDistance - p.refDistance;
  if (span <= 0) return 1;
  return Math.min(1, Math.max(0, 1 - (p.rolloffFactor * (d - p.refDistance)) / span));
}

/** O ouvinte está dentro do alcance do som? (sem raio = em todo o mapa) */
export function inRange(distance: number, radius: number | null): boolean {
  return radius === null || distance < radius;
}

export function distanceBetween(a: Vec3, b: Vec3): number {
  return Math.hypot(a[0] - b[0], a[1] - b[1], a[2] - b[2]);
}

/**
 * Volume que o usuário ouve de um som, de 0 a 1: volume da entidade (`health / 10`) x atenuação
 * por distância x volume mestre. Mudo (por som, solo ou global) zera.
 */
export function effectiveGain(o: { volume: number; distance: number; radius: number | null; master: number; silenced?: boolean }): number {
  if (o.silenced) return 0;
  const att = o.radius === null ? 1 : distanceGain(o.distance, pannerParams(o.radius));
  return clamp01(o.volume) * att * clamp01(o.master);
}

export const clamp01 = (v: number): number => (Number.isFinite(v) ? Math.min(1, Math.max(0, v)) : 0);

/** Ganho linear -> decibéis (`-Infinity` em silêncio total; piso de -120). */
export function gainToDb(gain: number): number {
  if (!(gain > 0)) return -120;
  return Math.max(-120, 20 * Math.log10(gain));
}

/** Medidor de nível: RMS linear (0..1) -> preenchimento da barra (-60 dB..0 dB -> 0..1). */
export function meterFill(rms: number, floorDb = -60): number {
  if (!(rms > 0)) return 0;
  const db = 20 * Math.log10(rms);
  return clamp01((db - floorDb) / -floorDb);
}

/** RMS de um bloco do `AnalyserNode.getByteTimeDomainData` (128 = silêncio). */
export function rmsOfBytes(data: ArrayLike<number>): number {
  if (!data.length) return 0;
  let acc = 0;
  for (let i = 0; i < data.length; i++) {
    const v = ((data[i] ?? 128) - 128) / 128;
    acc += v * v;
  }
  return Math.sqrt(acc / data.length);
}

// ------------------------------------------------------------------ mixagem (mute / solo)

export interface MixState {
  muted: ReadonlySet<number>;
  solo: ReadonlySet<number>;
  /** som em audição isolada, tocando inteiro e sem atenuação */
  preview: number | null;
  muteAll: boolean;
}

/**
 * O som entra na mixagem? Audição isolada manda sobre tudo; depois, se existe algum solo só os
 * solos tocam; senão, só os que não estão mudos. O mudo global silencia tudo (menos a audição isolada
 * deliberada, que é um pedido explícito do usuário).
 */
export function audibleInMix(id: number, m: MixState): boolean {
  if (m.preview !== null) return id === m.preview;
  if (m.muteAll) return false;
  if (m.solo.size > 0) return m.solo.has(id);
  return !m.muted.has(id);
}

// ------------------------------------------------------------------ seleção dos mais próximos

export interface Candidate {
  id: number;
  distance: number;
}

/**
 * Os `limit` candidatos mais próximos (empate -> menor `id`, para a escolha não oscilar).
 * Quem chama já filtrou o que está fora de alcance, mudo ou indisponível.
 */
export function selectNearest(cands: readonly Candidate[], limit: number): number[] {
  if (limit <= 0) return [];
  return [...cands]
    .sort((a, b) => a.distance - b.distance || a.id - b.id)
    .slice(0, limit)
    .map((c) => c.id);
}

// ------------------------------------------------------------------ loop

export interface LoopRegion {
  /** segundos */
  start: number;
  /** segundos; `0` = até o fim do arquivo (convenção do `AudioBufferSourceNode`) */
  end: number;
}

/**
 * Região de repetição do WAV (cue do GoldSrc ou `smpl`): usa a primeira. `null` = o arquivo não
 * tem laço (toca uma vez). Amostras -> segundos pela taxa do ARQUIVO (o buffer decodificado pode
 * ter outra taxa, mas o tempo é o mesmo).
 */
export function loopRegion(loops: readonly Pick<LoopPoint, "start_sample" | "end_sample">[], sampleRate: number, duration: number): LoopRegion | null {
  const lp = loops[0];
  if (!lp || !(sampleRate > 0)) return null;
  const start = lp.start_sample / sampleRate;
  if (!(start >= 0) || (duration > 0 && start >= duration)) return null;
  if (lp.end_sample === null) return { start, end: 0 };
  const end = lp.end_sample / sampleRate;
  if (!(end > start)) return { start, end: 0 };
  return { start, end: duration > 0 ? Math.min(end, duration) : end };
}

// ------------------------------------------------------------------ cache por memória

interface CacheEntry<T> {
  value: T;
  bytes: number;
  used: number;
}

/**
 * Cache com teto de bytes e remoção do menos usado recentemente. Entradas "presas" (som tocando
 * agora) não saem, mesmo que estourem o teto.
 */
export class BufferCache<T> {
  private readonly map = new Map<string, CacheEntry<T>>();
  private clock = 0;
  constructor(readonly maxBytes: number) {}

  get size(): number {
    return this.map.size;
  }

  get bytes(): number {
    let n = 0;
    for (const e of this.map.values()) n += e.bytes;
    return n;
  }

  has(key: string): boolean {
    return this.map.has(key);
  }

  get(key: string): T | undefined {
    const e = this.map.get(key);
    if (!e) return undefined;
    e.used = ++this.clock;
    return e.value;
  }

  /** guarda e remove o excedente; devolve as chaves removidas */
  put(key: string, value: T, bytes: number, pinned: (key: string) => boolean = () => false): string[] {
    this.map.set(key, { value, bytes, used: ++this.clock });
    return this.trim(pinned, key);
  }

  trim(pinned: (key: string) => boolean = () => false, keep?: string): string[] {
    const removed: string[] = [];
    let total = this.bytes;
    if (total <= this.maxBytes) return removed;
    const order = [...this.map.entries()].sort((a, b) => a[1].used - b[1].used);
    for (const [key, e] of order) {
      if (total <= this.maxBytes) break;
      if (key === keep || pinned(key)) continue;
      this.map.delete(key);
      total -= e.bytes;
      removed.push(key);
    }
    return removed;
  }

  clear(): void {
    this.map.clear();
  }
}

// ------------------------------------------------------------------ transporte

/** `data:<mime>;base64,...` -> bytes (o `read_mod_audio` entrega assim). */
export function dataUrlToBytes(url: string): ArrayBuffer {
  const comma = url.indexOf(",");
  if (!url.startsWith("data:") || comma < 0) throw new Error("data URL inválida");
  const bin = atob(url.slice(comma + 1));
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out.buffer;
}

// ------------------------------------------------------------------ cores

/** Cor do alcance de um som (distribui matizes pelo ângulo áureo: vizinhos ficam distintos). */
export function soundHue(id: number): number {
  return (id * 137.508) % 360;
}

export function soundColor(id: number): string {
  return `hsl(${Math.round(soundHue(id))} 75% 58%)`;
}

// ------------------------------------------------------------------ motor (Web Audio)

/** Por que um som não toca. `detail` é texto cru do backend/navegador, quando há. */
export type UnavailableReason = "noFile" | "sentence" | "missing" | "codec" | "decode" | "tooBig";

export interface Unavailable {
  code: UnavailableReason;
  detail?: string;
}

export type SoundStatus = "idle" | "loading" | "playing" | "done" | "unavailable";

export interface SoundState {
  id: number;
  status: SoundStatus;
  reason?: Unavailable;
  /** distância atual do ouvinte (unidades) */
  distance: number;
  inRange: boolean;
  /** volume efetivo 0..1 (volume x atenuação x mestre; 0 se silenciado) */
  gain: number;
  muted: boolean;
  solo: boolean;
  /** fora da mixagem agora (mudo, mudo global, outro som em solo/audição isolada, inicia silencioso) */
  silenced: boolean;
  previewing: boolean;
  /** nível do sinal (0..1, escala em dB) enquanto toca */
  level: number;
  /** o arquivo tem laço (cue/smpl) */
  loops: boolean;
  duration: number | null;
}

/** O que o motor pede ao app para ler um arquivo de som. */
export interface AudioBackend {
  analyze(rel: string): Promise<AudioInfo>;
  read(rel: string): Promise<ArrayBuffer>;
}

interface Loaded {
  buffer: AudioBuffer;
  info: AudioInfo;
}

interface Probe {
  info?: AudioInfo;
  fail?: Unavailable;
}

interface Voice {
  src: AudioBufferSourceNode;
  gain: GainNode;
  panner: PannerNode | null;
  analyser: AnalyserNode;
  data: Uint8Array<ArrayBuffer>;
  flat: boolean;
  rel: string;
  ended: boolean;
}

interface Runtime {
  voice: Voice | null;
  /** toca até o fim sem laço e terminou */
  done: boolean;
  loading: boolean;
  level: number;
}

const errText = (e: unknown): string => (e instanceof Error ? e.message : typeof e === "string" ? e : JSON.stringify(e));

export class SpatialAudio {
  private ctx: AudioContext | null = null;
  private master: GainNode | null = null;
  private disposed = false;

  private masterVolume = 0.8;
  private muteAll = false;
  private repeatAll = true;
  private maxVoices = MAX_VOICES;
  private readonly mutes = new Set<number>();
  private readonly solos = new Set<number>();
  private preview: number | null = null;

  private readonly byId = new Map<number, SoundSource>();
  private readonly rt = new Map<number, Runtime>();
  /** o que se sabe de cada arquivo antes de decodificar (análise ou falha) */
  private readonly probes = new Map<string, Probe>();
  private readonly cache: BufferCache<Loaded>;
  private readonly loading = new Map<string, Promise<Loaded | Unavailable>>();
  private readonly failed = new Map<string, Unavailable>();
  private readonly pending: (() => void)[] = [];
  private activeLoads = 0;
  private probeAbort = false;

  private listener: { pos: Vec3; fwd: Vec3; up: Vec3 } = { pos: [0, 0, 0], fwd: [0, 0, -1], up: [0, 1, 0] };
  private lastSelect = -Infinity;
  private changed: (() => void) | null = null;

  constructor(
    readonly sources: readonly SoundSource[],
    private readonly backend: AudioBackend,
    maxCacheBytes = MAX_CACHE_BYTES,
  ) {
    this.cache = new BufferCache<Loaded>(maxCacheBytes);
    for (const s of sources) {
      this.byId.set(s.id, s);
      this.rt.set(s.id, { voice: null, done: false, loading: false, level: 0 });
      const f = soundRel(s.file);
      if (!f.ok) this.failed.set(this.keyOf(s), { code: f.reason });
    }
  }

  // ---------------------------------------------------------------- ciclo de vida

  get enabled(): boolean {
    return this.ctx !== null && !this.disposed;
  }

  /** Estado do `AudioContext` (`null` antes de ativar). */
  get contextState(): AudioContextState | null {
    return this.ctx?.state ?? null;
  }

  onChange(fn: () => void): void {
    this.changed = fn;
  }

  /**
   * Cria o contexto. PRECISA ser chamado dentro de um gesto do usuário (clique no botão
   * "Ativar áudio"): sem isso o navegador deixa o contexto suspenso.
   */
  async enable(): Promise<void> {
    if (this.disposed) return;
    if (!this.ctx) {
      const Ctor: typeof AudioContext | undefined = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext;
      if (!Ctor) throw new Error("Web Audio indisponível");
      this.ctx = new Ctor({ latencyHint: "interactive" });
      this.master = this.ctx.createGain();
      this.master.gain.value = this.muteAll ? 0 : this.masterVolume;
      this.master.connect(this.ctx.destination);
    }
    this.probe();
    if (this.ctx.state === "suspended") await this.ctx.resume();
    this.lastSelect = -Infinity;
    this.changed?.();
  }

  /** Pausa o contexto (camada desligada): as vozes ficam congeladas, sem custo de CPU. */
  async pause(): Promise<void> {
    if (this.ctx && this.ctx.state === "running") await this.ctx.suspend();
    this.changed?.();
  }

  /** Para tudo e solta o contexto (nada fica tocando nem preso na memória). */
  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.probeAbort = true;
    for (const id of [...this.rt.keys()]) this.stopVoice(id, true);
    this.pending.length = 0;
    this.cache.clear();
    this.loading.clear();
    this.probes.clear();
    this.master?.disconnect();
    const ctx = this.ctx;
    this.ctx = null;
    this.master = null;
    if (ctx && ctx.state !== "closed") void ctx.close().catch(() => undefined);
    this.changed = null;
  }

  // ---------------------------------------------------------------- controles

  setMasterVolume(v: number): void {
    this.masterVolume = clamp01(v);
    this.applyMaster();
  }

  getMasterVolume(): number {
    return this.masterVolume;
  }

  setMuteAll(on: boolean): void {
    this.muteAll = on;
    this.applyMaster();
    this.lastSelect = -Infinity;
  }

  isMuteAll(): boolean {
    return this.muteAll;
  }

  /** repete também os sons sem laço (senão tocam uma vez e calam) */
  setRepeatAll(on: boolean): void {
    this.repeatAll = on;
    for (const r of this.rt.values()) r.done = false;
    for (const id of this.rt.keys()) this.stopVoice(id);
    this.lastSelect = -Infinity;
  }

  isRepeatAll(): boolean {
    return this.repeatAll;
  }

  setMaxVoices(n: number): void {
    this.maxVoices = Math.max(1, Math.floor(n));
    this.lastSelect = -Infinity;
  }

  setMuted(id: number, on: boolean): void {
    if (on) this.mutes.add(id);
    else this.mutes.delete(id);
    this.lastSelect = -Infinity;
  }

  setSolo(id: number, on: boolean): void {
    if (on) this.solos.add(id);
    else this.solos.delete(id);
    this.lastSelect = -Infinity;
  }

  /** Audição isolada: toca só este som, inteiro e sem atenuação. `null` encerra. */
  setPreview(id: number | null): void {
    if (this.preview === id) return;
    const prev = this.preview;
    this.preview = id;
    if (prev !== null) this.stopVoice(prev);
    if (id !== null) {
      const r = this.rt.get(id);
      if (r) r.done = false;
      this.stopVoice(id); // recria sem espacialização
    }
    this.lastSelect = -Infinity;
  }

  getPreview(): number | null {
    return this.preview;
  }

  isMuted(id: number): boolean {
    return this.mutes.has(id);
  }

  isSolo(id: number): boolean {
    return this.solos.has(id);
  }

  private applyMaster(): void {
    if (!this.master || !this.ctx) return;
    this.master.gain.setTargetAtTime(this.muteAll ? 0 : this.masterVolume, this.ctx.currentTime, 0.02);
  }

  private mix(): MixState {
    return { muted: this.mutes, solo: this.solos, preview: this.preview, muteAll: this.muteAll };
  }

  // ---------------------------------------------------------------- ouvinte e quadro

  /**
   * Atualiza o ouvinte (câmera) e, a cada `SELECT_INTERVAL_MS`, reavalia quais sons tocam.
   * `pos`/`fwd`/`up` já em coordenadas da cena (Y-up).
   */
  update(pos: Vec3, fwd: Vec3, up: Vec3, nowMs: number): void {
    this.listener = { pos, fwd, up };
    const ctx = this.ctx;
    if (!ctx || this.disposed) return;
    const l = ctx.listener;
    if (l.positionX) {
      l.positionX.value = pos[0];
      l.positionY.value = pos[1];
      l.positionZ.value = pos[2];
      l.forwardX.value = fwd[0];
      l.forwardY.value = fwd[1];
      l.forwardZ.value = fwd[2];
      l.upX.value = up[0];
      l.upY.value = up[1];
      l.upZ.value = up[2];
    } else {
      // Safari/Firefox antigos
      const legacy = l as unknown as { setPosition(x: number, y: number, z: number): void; setOrientation(...a: number[]): void };
      legacy.setPosition(pos[0], pos[1], pos[2]);
      legacy.setOrientation(fwd[0], fwd[1], fwd[2], up[0], up[1], up[2]);
    }
    if (nowMs - this.lastSelect >= SELECT_INTERVAL_MS) {
      this.lastSelect = nowMs;
      this.reselect();
    }
  }

  private distanceTo(s: SoundSource): number {
    return distanceBetween(this.listener.pos, sourcePosition(s));
  }

  private reselect(): void {
    if (!this.ctx || this.ctx.state !== "running") return;
    const mix = this.mix();
    const cands: Candidate[] = [];
    for (const s of this.sources) {
      const unavailable = this.unavailableOf(s);
      if (unavailable) continue;
      const forced = this.preview === s.id;
      if (!audibleInMix(s.id, mix)) continue;
      // começa silencioso: só entra pelo solo ou pela audição isolada
      if (s.startSilent && !forced && !this.solos.has(s.id)) continue;
      const rt = this.rt.get(s.id)!;
      if (rt.done && !forced) continue;
      const d = forced || s.radius === null ? 0 : this.distanceTo(s);
      if (!forced && !inRange(d, s.radius)) continue;
      cands.push({ id: s.id, distance: d });
    }
    const chosen = new Set(selectNearest(cands, this.maxVoices));
    for (const id of this.rt.keys()) {
      if (!chosen.has(id)) this.stopVoice(id);
    }
    for (const id of chosen) this.ensureVoice(id);
  }

  // ---------------------------------------------------------------- vozes

  private ensureVoice(id: number): void {
    const s = this.byId.get(id)!;
    const r = this.rt.get(id)!;
    if (r.voice || r.loading) return;
    const f = soundRel(s.file);
    if (!f.ok) return;
    const hit = this.cache.get(f.rel);
    if (hit) {
      this.startVoice(s, r, f.rel, hit);
      return;
    }
    r.loading = true;
    void this.load(f.rel).then((res) => {
      r.loading = false;
      if (this.disposed) return;
      this.changed?.();
      if ("buffer" in res) {
        // ainda é um dos escolhidos? a próxima reavaliação decide; força já
        this.lastSelect = -Infinity;
      }
    });
  }

  private startVoice(s: SoundSource, r: Runtime, rel: string, loaded: Loaded): void {
    const ctx = this.ctx;
    const master = this.master;
    if (!ctx || !master || r.voice) return;
    const flat = s.radius === null || this.preview === s.id;
    const src = ctx.createBufferSource();
    src.buffer = loaded.buffer;
    const region = loopRegion(loaded.info.loops, loaded.info.sample_rate, loaded.info.duration_secs);
    const loops = region !== null || this.repeatAll || this.preview === s.id;
    if (loops) {
      src.loop = true;
      if (region) {
        src.loopStart = region.start;
        src.loopEnd = region.end;
      }
    }
    const gain = ctx.createGain();
    gain.gain.value = 0;
    const analyser = ctx.createAnalyser();
    analyser.fftSize = 256;
    let panner: PannerNode | null = null;
    src.connect(gain);
    if (flat) {
      gain.connect(analyser);
    } else {
      const p = pannerParams(s.radius!);
      panner = ctx.createPanner();
      panner.panningModel = "HRTF";
      panner.distanceModel = "linear";
      panner.refDistance = p.refDistance;
      panner.maxDistance = p.maxDistance;
      panner.rolloffFactor = p.rolloffFactor;
      const [x, y, z] = sourcePosition(s);
      panner.positionX.value = x;
      panner.positionY.value = y;
      panner.positionZ.value = z;
      gain.connect(panner);
      panner.connect(analyser);
    }
    analyser.connect(master);
    const voice: Voice = { src, gain, panner, analyser, data: new Uint8Array(analyser.fftSize), flat, rel, ended: false };
    src.onended = () => {
      voice.ended = true;
      if (r.voice === voice) {
        r.voice = null;
        r.done = !src.loop;
        this.disconnect(voice);
        this.changed?.();
      }
    };
    r.voice = voice;
    gain.gain.setTargetAtTime(clamp01(s.volume), ctx.currentTime, 0.03);
    src.start();
    this.changed?.();
  }

  private stopVoice(id: number, now = false): void {
    const r = this.rt.get(id);
    const v = r?.voice;
    if (!r || !v) return;
    r.voice = null;
    r.level = 0;
    const ctx = this.ctx;
    v.src.onended = null;
    if (ctx && !now) {
      // rampa curta: cortar no meio da onda estala
      v.gain.gain.cancelScheduledValues(ctx.currentTime);
      v.gain.gain.setTargetAtTime(0, ctx.currentTime, 0.015);
      try {
        v.src.stop(ctx.currentTime + 0.08);
      } catch {
        /* já parou */
      }
      v.src.onended = () => this.disconnect(v);
    } else {
      try {
        v.src.stop();
      } catch {
        /* já parou */
      }
      this.disconnect(v);
    }
  }

  private disconnect(v: Voice): void {
    v.src.onended = null;
    try {
      v.src.disconnect();
      v.gain.disconnect();
      v.panner?.disconnect();
      v.analyser.disconnect();
    } catch {
      /* nó já solto */
    }
  }

  // ---------------------------------------------------------------- carga de arquivos

  private keyOf(s: SoundSource): string {
    const f = soundRel(s.file);
    return f.ok ? f.rel : `#${s.id}`;
  }

  private unavailableOf(s: SoundSource): Unavailable | undefined {
    const f = soundRel(s.file);
    if (!f.ok) return { code: f.reason };
    return this.failed.get(f.rel) ?? this.probes.get(f.rel)?.fail;
  }

  private enqueue<T>(job: () => Promise<T>): Promise<T> {
    return new Promise<T>((resolve, reject) => {
      const run = () => {
        this.activeLoads++;
        job().then(resolve, reject).finally(() => {
          this.activeLoads--;
          this.pending.shift()?.();
        });
      };
      if (this.activeLoads < MAX_LOADS) run();
      else this.pending.push(run);
    });
  }

  /** Análise do arquivo (cacheada); falhas viram `Unavailable`. */
  private async inspect(rel: string): Promise<Probe> {
    const known = this.probes.get(rel);
    if (known) return known;
    let probe: Probe;
    try {
      const info = await this.backend.analyze(rel);
      probe = info.playable ? { info } : { info, fail: { code: "codec", detail: info.codec } };
    } catch (e) {
      probe = { fail: { code: "missing", detail: errText(e) } };
    }
    if (!this.disposed) {
      this.probes.set(rel, probe);
      if (probe.fail) this.failed.set(rel, probe.fail);
    }
    return probe;
  }

  /**
   * Sondagem em segundo plano: analisa os arquivos (sem decodificar) para a lista marcar cedo quem
   * não vai tocar (ausente, ADPCM) e quem tem laço. Não precisa de gesto do usuário; roda uma vez.
   */
  probe(): void {
    if (this.probeStarted || this.disposed) return;
    this.probeStarted = true;
    void this.probeAll();
  }

  private probeStarted = false;

  private async probeAll(): Promise<void> {
    const rels: string[] = [];
    for (const s of this.sources) {
      const f = soundRel(s.file);
      if (f.ok && !rels.includes(f.rel)) rels.push(f.rel);
      if (rels.length >= MAX_PROBES) break;
    }
    for (const rel of rels) {
      if (this.probeAbort || this.disposed) return;
      if (!this.probes.has(rel)) await this.enqueue(() => this.inspect(rel));
      this.changed?.();
    }
  }

  private load(rel: string): Promise<Loaded | Unavailable> {
    const going = this.loading.get(rel);
    if (going) return going;
    const job = this.enqueue(async (): Promise<Loaded | Unavailable> => {
      const probe = await this.inspect(rel);
      if (probe.fail || !probe.info) return probe.fail ?? { code: "missing" };
      const ctx = this.ctx;
      if (!ctx || this.disposed) return { code: "decode" };
      let bytes: ArrayBuffer;
      try {
        bytes = await this.backend.read(rel);
      } catch (e) {
        const fail: Unavailable = { code: /grande|large|exceed|limite/i.test(errText(e)) ? "tooBig" : "missing", detail: errText(e) };
        this.failed.set(rel, fail);
        return fail;
      }
      try {
        const buffer = await ctx.decodeAudioData(bytes);
        const loaded: Loaded = { buffer, info: probe.info };
        if (!this.disposed) {
          this.cache.put(rel, loaded, buffer.length * buffer.numberOfChannels * 4, (k) => this.isPlaying(k));
        }
        return loaded;
      } catch (e) {
        const fail: Unavailable = { code: "decode", detail: errText(e) };
        this.failed.set(rel, fail);
        return fail;
      }
    });
    this.loading.set(rel, job);
    void job.finally(() => this.loading.delete(rel));
    return job;
  }

  private isPlaying(rel: string): boolean {
    for (const r of this.rt.values()) if (r.voice?.rel === rel) return true;
    return false;
  }

  // ---------------------------------------------------------------- consulta (lista, medidor)

  /** Cópia do estado de todos os sons (a lista lateral chama ~10x por segundo). */
  states(): SoundState[] {
    const mix = this.mix();
    const out: SoundState[] = [];
    for (const s of this.sources) {
      const r = this.rt.get(s.id)!;
      const un = this.unavailableOf(s);
      const f = soundRel(s.file);
      const probe = f.ok ? this.probes.get(f.rel) : undefined;
      const distance = this.distanceTo(s);
      const forced = this.preview === s.id;
      const silenced = !audibleInMix(s.id, mix) || (s.startSilent && !forced && !this.solos.has(s.id));
      const gain = effectiveGain({
        volume: s.volume,
        distance: forced ? 0 : distance,
        radius: forced ? null : s.radius,
        master: this.masterVolume,
        silenced,
      });
      let level = 0;
      if (r.voice && !r.voice.ended) {
        r.voice.analyser.getByteTimeDomainData(r.voice.data);
        level = meterFill(rmsOfBytes(r.voice.data));
      }
      r.level = level;
      const status: SoundStatus = un ? "unavailable" : r.voice ? "playing" : r.loading ? "loading" : r.done ? "done" : "idle";
      out.push({
        id: s.id,
        status,
        reason: un,
        distance,
        inRange: inRange(distance, s.radius),
        gain,
        muted: this.mutes.has(s.id),
        solo: this.solos.has(s.id),
        silenced,
        previewing: forced,
        level,
        loops: probe?.info ? loopRegion(probe.info.loops, probe.info.sample_rate, probe.info.duration_secs) !== null : false,
        duration: probe?.info?.duration_secs ?? null,
      });
    }
    return out;
  }

  /** Quantos sons estão tocando agora. */
  activeCount(): number {
    let n = 0;
    for (const r of this.rt.values()) if (r.voice) n++;
    return n;
  }

  /** Bytes de áudio decodificado em cache. */
  cacheBytes(): number {
    return this.cache.bytes;
  }
}
