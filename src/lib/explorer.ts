// Partes puras do Explorador (sem DOM, sem Tauri): formatação, filtro/ordenação da lista de
// arquivos do mod, árvore de pastas, janela da lista virtual, forma de onda e linha do tempo
// de sprites. Tudo aqui tem teste em explorer.test.ts.

import type { FileKind, Layer, ModFileEntry } from "../types/modfiles.ts";
import type { SpriteFrame } from "../types/formats.ts";

// ------------------------------------------------------------------ formatação

/** Tamanho legível: `512 B`, `3,4 KB`, `120 KB`, `1,2 MB`, `2,0 GB` (1 casa decimal abaixo de 10). */
export function formatBytes(bytes: number, locale = "pt-BR"): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "–";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  if (unit === 0) return `${Math.round(value)} B`;
  const digits = value < 10 ? 1 : 0;
  return `${value.toLocaleString(locale, { minimumFractionDigits: digits, maximumFractionDigits: digits })} ${units[unit]}`;
}

/** `m:ss,cc` (ou `h:mm:ss`) a partir de segundos. */
export function formatDuration(secs: number): string {
  if (!Number.isFinite(secs) || secs < 0) return "–";
  const total = Math.round(secs * 100);
  const cs = total % 100;
  const s = Math.floor(total / 100) % 60;
  const m = Math.floor(total / 6000) % 60;
  const h = Math.floor(total / 360000);
  const two = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${two(m)}:${two(s)}` : `${m}:${two(s)}.${two(cs)}`;
}

/** Nome de arquivo seguro para o download (`{LAVA1` -> `_LAVA1`). */
export function safeFileName(name: string): string {
  return name.replace(/[^\w.-]+/g, "_") || "arquivo";
}

/** Pico (0..1) em dBFS; `null` em silêncio total. */
export function peakDb(peak: number | null): number | null {
  if (peak == null || !(peak > 0)) return null;
  return 20 * Math.log10(Math.min(peak, 1));
}

// ------------------------------------------------------------------ caminhos

/** `<mod>/maps` (ou `\maps`) -> `<mod>`; qualquer outra pasta é a própria pasta do mod. */
export function modRootFromMapsDir(dir: string): string {
  const trimmed = dir.replace(/[\\/]+$/, "");
  const m = /^(.*)[\\/]maps$/i.exec(trimmed);
  return m && m[1] ? m[1] : trimmed;
}

/** Junta raiz e caminho relativo (`/`) usando o separador que a raiz já usa. */
export function joinPath(root: string, rel: string): string {
  const sep = root.includes("\\") ? "\\" : "/";
  const base = root.replace(/[\\/]+$/, "");
  const tail = rel.replace(/^[\\/]+/, "");
  return `${base}${sep}${sep === "\\" ? tail.replace(/\//g, "\\") : tail}`;
}

export function baseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

export function dirName(path: string): string {
  const i = path.lastIndexOf("/");
  return i < 0 ? "" : path.slice(0, i);
}

export function extOf(name: string): string {
  const i = name.lastIndexOf(".");
  return i <= 0 ? "" : name.slice(i + 1).toLowerCase();
}

/** Mesma regra do backend (`modfiles::classify`), mais `.lmp` como imagem (o visualizador abre). */
export function classifyName(name: string): FileKind {
  switch (extOf(name)) {
    case "bsp":
      return "map";
    case "wad":
      return "wad";
    case "mdl":
      return "model";
    case "spr":
      return "sprite";
    case "wav":
    case "mp3":
    case "ogg":
      return "sound";
    case "bmp":
    case "tga":
    case "png":
    case "jpg":
    case "jpeg":
    case "gif":
    case "pcx":
    case "dds":
    case "lmp":
      return "image";
    case "nav":
      return "nav";
    case "dem":
      return "demo";
    case "txt":
    case "res":
    case "lst":
    case "log":
    case "html":
    case "htm":
    case "xml":
    case "md":
    case "json":
    case "csv":
    case "inf":
      return "text";
    case "pak":
      return "pak";
    case "dll":
    case "so":
    case "dylib":
    case "exe":
      return "dll";
    case "cfg":
    case "rc":
    case "ini":
    case "gam":
    case "scr":
    case "vdf":
      return "cfg";
    default:
      return "other";
  }
}

/** Tipo efetivo de uma entrada do backend (`.lmp` vem como `other`, mas abre como imagem). */
export function effectiveKind(e: Pick<ModFileEntry, "name" | "kind">): FileKind {
  return e.kind === "other" ? classifyName(e.name) : e.kind;
}

// ------------------------------------------------------------------ filtro e ordenação

/** Chave única de uma entrada (o mesmo caminho pode existir no mod, no valve e dentro de um pak). */
export function entryKey(e: Pick<ModFileEntry, "path" | "pak" | "layer" | "origin">): string {
  return `${e.origin}:${e.layer}:${e.pak ?? ""}:${e.path}`;
}

export type FileSort = "path" | "name" | "size" | "kind";

export interface FileFilter {
  /** termos separados por espaço; todos precisam aparecer no caminho (sem diferenciar maiúsculas) */
  query: string;
  /** "" = qualquer tipo */
  kind: FileKind | "";
  /** "" = qualquer camada */
  layer: Layer | "";
  sort: FileSort;
  /** ordem decrescente */
  desc: boolean;
}

export const EMPTY_FILE_FILTER: FileFilter = { query: "", kind: "", layer: "", sort: "path", desc: false };

export function matchesQuery(entry: Pick<ModFileEntry, "path" | "pak">, query: string): boolean {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!terms.length) return true;
  const hay = (entry.pak ? `${entry.pak}/${entry.path}` : entry.path).toLowerCase();
  return terms.every((term) => hay.includes(term));
}

export function filterEntries(entries: readonly ModFileEntry[], f: Pick<FileFilter, "query" | "kind" | "layer">): ModFileEntry[] {
  return entries.filter(
    (e) => (!f.kind || effectiveKind(e) === f.kind) && (!f.layer || e.layer === f.layer) && matchesQuery(e, f.query),
  );
}

const LAYER_RANK: Record<Layer, number> = { mod: 0, valve: 1, pak: 2 };

/** Ordenação estável e determinística (empates caem no caminho, depois na camada). */
export function sortEntries(entries: readonly ModFileEntry[], sort: FileSort, desc = false): ModFileEntry[] {
  const text = (a: string, b: string) => a.localeCompare(b, "en", { numeric: true, sensitivity: "base" });
  const byPath = (a: ModFileEntry, b: ModFileEntry) =>
    text(a.path, b.path) || LAYER_RANK[a.layer] - LAYER_RANK[b.layer] || text(a.pak ?? "", b.pak ?? "");
  const cmp = (a: ModFileEntry, b: ModFileEntry): number => {
    switch (sort) {
      case "name":
        return text(a.name, b.name) || byPath(a, b);
      case "size":
        return a.size - b.size || byPath(a, b);
      case "kind":
        return text(effectiveKind(a), effectiveKind(b)) || byPath(a, b);
      default:
        return byPath(a, b);
    }
  };
  const out = [...entries].sort(cmp);
  return desc ? out.reverse() : out;
}

export function filterAndSort(entries: readonly ModFileEntry[], f: FileFilter): ModFileEntry[] {
  return sortEntries(filterEntries(entries, f), f.sort, f.desc);
}

/** Soma de tamanho e contagem por tipo (para os chips de resumo depois de filtrar). */
export function totalsByKind(entries: readonly ModFileEntry[]): Map<FileKind, { count: number; bytes: number }> {
  const out = new Map<FileKind, { count: number; bytes: number }>();
  for (const e of entries) {
    const k = effectiveKind(e);
    const cur = out.get(k) ?? { count: 0, bytes: 0 };
    cur.count++;
    cur.bytes += e.size;
    out.set(k, cur);
  }
  return out;
}

// ------------------------------------------------------------------ árvore

export interface TreeNode {
  name: string;
  /** caminho da pasta (sem `/` final); arquivos: caminho da entrada */
  path: string;
  isDir: boolean;
  size: number;
  fileCount: number;
  entry: ModFileEntry | null;
  children: TreeNode[];
}

/**
 * Monta a árvore de pastas. Itens de pak entram sob uma pasta com o nome do pak
 * (`pak0.pak/sound/x.wav`), que é também o caminho aceito por `read_mod_file`.
 */
export function buildTree(entries: readonly ModFileEntry[]): TreeNode {
  const root: TreeNode = { name: "", path: "", isDir: true, size: 0, fileCount: 0, entry: null, children: [] };
  const dirs = new Map<string, TreeNode>([["", root]]);
  const dirOf = (path: string): TreeNode => {
    const hit = dirs.get(path);
    if (hit) return hit;
    const parent = dirOf(dirName(path));
    const node: TreeNode = { name: baseName(path), path, isDir: true, size: 0, fileCount: 0, entry: null, children: [] };
    parent.children.push(node);
    dirs.set(path, node);
    return node;
  };
  for (const e of entries) {
    const full = e.pak ? `${e.pak}/${e.path}` : e.path;
    const parent = dirOf(dirName(full));
    parent.children.push({ name: e.name, path: full, isDir: false, size: e.size, fileCount: 1, entry: e, children: [] });
    let d: TreeNode = parent;
    for (;;) {
      d.size += e.size;
      d.fileCount++;
      if (d === root) break;
      d = dirs.get(dirName(d.path)) ?? root;
    }
  }
  const order = (n: TreeNode) => {
    // pastas primeiro, depois por nome
    n.children.sort(
      (a, b) => Number(b.isDir) - Number(a.isDir) || a.name.localeCompare(b.name, "en", { numeric: true, sensitivity: "base" }),
    );
    n.children.forEach(order);
  };
  order(root);
  return root;
}

export interface TreeRow {
  node: TreeNode;
  depth: number;
}

/** Linhas visíveis da árvore: pastas em `open` (ou todas, com `openAll`) mostram os filhos. */
export function flattenTree(root: TreeNode, open: ReadonlySet<string>, openAll = false): TreeRow[] {
  const rows: TreeRow[] = [];
  const walk = (n: TreeNode, depth: number) => {
    for (const c of n.children) {
      rows.push({ node: c, depth });
      if (c.isDir && (openAll || open.has(c.path))) walk(c, depth + 1);
    }
  };
  walk(root, 0);
  return rows;
}

// ------------------------------------------------------------------ lista virtual

/** Janela [first, last) de linhas a desenhar para um scroll; `overscan` linhas de folga. */
export function visibleRange(scrollTop: number, viewportH: number, rowH: number, total: number, overscan = 6): [number, number] {
  if (total <= 0 || rowH <= 0) return [0, 0];
  const last = Math.min(total, Math.ceil((scrollTop + viewportH) / rowH) + overscan);
  const first = Math.min(last, Math.max(0, Math.floor(scrollTop / rowH) - overscan));
  return [first, last];
}

// ------------------------------------------------------------------ forma de onda

export type Envelope = readonly (readonly [number, number])[];

/**
 * Reduz o envelope a no máximo `columns` colunas (min dos mínimos, max dos máximos) — com mais
 * trechos que pixels, desenhar todos só borra.
 */
export function reduceEnvelope(env: Envelope, columns: number): [number, number][] {
  const n = env.length;
  const cols = Math.max(1, Math.floor(columns));
  if (n <= cols) return env.map(([a, b]) => [a, b]);
  const out: [number, number][] = [];
  for (let c = 0; c < cols; c++) {
    const from = Math.floor((c * n) / cols);
    const to = Math.max(from + 1, Math.floor(((c + 1) * n) / cols));
    let lo = 1;
    let hi = -1;
    for (let i = from; i < to && i < n; i++) {
      const [a, b] = env[i]!;
      if (a < lo) lo = a;
      if (b > hi) hi = b;
    }
    out.push([lo, hi]);
  }
  return out;
}

/**
 * Polígono da forma de onda em pixels: borda de cima (máximos) da esquerda para a direita e
 * de baixo (mínimos) da direita para a esquerda, em escala cheia (±1) centrada em `height / 2`.
 * `gain` amplia (o envelope é "escala cheia", então um áudio baixo fica achatado).
 */
export function waveformPolyline(env: Envelope, width: number, height: number, gain = 1): [number, number][] {
  if (!env.length || width <= 0 || height <= 0) return [];
  const cols = reduceEnvelope(env, Math.max(1, Math.floor(width)));
  const mid = height / 2;
  const clamp = (v: number) => Math.max(-1, Math.min(1, v * gain));
  const x = (i: number) => (cols.length === 1 ? 0 : (i * width) / (cols.length - 1));
  const top: [number, number][] = cols.map(([, hi], i) => [x(i), mid - clamp(hi) * mid]);
  const bottom: [number, number][] = cols.map(([lo], i) => [x(i), mid - clamp(lo) * mid]).reverse() as [number, number][];
  return [...top, ...bottom];
}

/** Ganho que leva o pico a ~90% da altura (limitado a 8×; sem pico = 1). */
export function autoGain(peak: number | null): number {
  if (peak == null || !(peak > 0)) return 1;
  return Math.max(1, Math.min(8, 0.9 / peak));
}

export function secondsToX(secs: number, duration: number, width: number): number {
  if (!(duration > 0)) return 0;
  return Math.max(0, Math.min(1, secs / duration)) * width;
}

export function xToSeconds(x: number, duration: number, width: number): number {
  if (!(width > 0)) return 0;
  return Math.max(0, Math.min(1, x / width)) * duration;
}

export interface LoopSpan {
  start: number;
  /** segundos; fim do arquivo quando o loop não declara fim */
  end: number;
  source: string;
}

/** Loops do backend (em frames de amostra) -> segundos. */
export function loopSpans(
  loops: readonly { start_sample: number; end_sample: number | null; source: string }[],
  sampleRate: number,
  duration: number,
): LoopSpan[] {
  if (!(sampleRate > 0)) return [];
  return loops.map((l) => ({
    start: l.start_sample / sampleRate,
    end: l.end_sample == null ? duration : l.end_sample / sampleRate,
    source: l.source,
  }));
}

// ------------------------------------------------------------------ sprites

export interface SpriteSegment {
  /** índice do frame no arquivo */
  frame: number;
  /** índice da imagem dentro do frame (0 em frame simples) */
  image: number;
  start: number;
  duration: number;
}

/** Duração padrão de uma imagem de grupo sem tempo válido. */
export const DEFAULT_GROUP_STEP = 0.1;

/**
 * Linha do tempo da animação: frame simples dura `1 / fps`; imagem de grupo dura o intervalo
 * que o arquivo declara (o motor usa esses tempos, não o fps).
 */
export function buildTimeline(frames: readonly Pick<SpriteFrame, "kind" | "durations" | "images">[], fps: number): SpriteSegment[] {
  const single = 1 / Math.max(0.1, fps);
  const out: SpriteSegment[] = [];
  let t = 0;
  frames.forEach((f, frame) => {
    if (f.kind === "group") {
      f.images.forEach((_, image) => {
        const d = f.durations[image];
        const duration = d != null && d > 0 ? d : DEFAULT_GROUP_STEP;
        out.push({ frame, image, start: t, duration });
        t += duration;
      });
    } else {
      out.push({ frame, image: 0, start: t, duration: single });
      t += single;
    }
  });
  return out;
}

export function timelineDuration(timeline: readonly SpriteSegment[]): number {
  const last = timeline[timeline.length - 1];
  return last ? last.start + last.duration : 0;
}

/** Índice do segmento exibido no instante `t` (com `loop`, o tempo dá a volta; senão fica no último). */
export function segmentAt(timeline: readonly SpriteSegment[], t: number, loop = true): number {
  if (!timeline.length) return -1;
  const total = timelineDuration(timeline);
  if (!(total > 0)) return 0;
  let time = t;
  if (loop) time = ((t % total) + total) % total;
  else time = Math.max(0, Math.min(t, total - 1e-9));
  // busca binária: último segmento com start <= time
  let lo = 0;
  let hi = timeline.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (timeline[mid]!.start <= time) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

// ------------------------------------------------------------------ hex

/** Despejo hexadecimal clássico: `00000000  48 65 …  |He…|`. */
export function hexDump(bytes: Uint8Array, perLine = 16, baseOffset = 0): string {
  const lines: string[] = [];
  for (let off = 0; off < bytes.length; off += perLine) {
    const chunk = bytes.subarray(off, off + perLine);
    const hex = Array.from(chunk, (b) => b.toString(16).padStart(2, "0"));
    while (hex.length < perLine) hex.push("  ");
    const ascii = Array.from(chunk, (b) => (b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : ".")).join("");
    const mid = perLine / 2;
    lines.push(
      `${(baseOffset + off).toString(16).padStart(8, "0")}  ${hex.slice(0, mid).join(" ")}  ${hex.slice(mid).join(" ")}  |${ascii}|`,
    );
  }
  return lines.join("\n");
}

export function base64ToBytes(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}
