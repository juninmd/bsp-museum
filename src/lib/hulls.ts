/**
 * Paginação dos hulls de colisão. O backend devolve o hull em janelas (`map_hulls` com
 * `offset`/`limit`); o frontend segue `next_offset` até acabar, com tetos de segurança
 * (triângulos e quantidade de janelas) para um mapa hostil não travar a GPU.
 *
 * Código puro: o `invoke` fica com quem chama; aqui só o planejamento e a contabilidade.
 */

/** Triângulos pedidos por janela. */
export const HULL_WINDOW = 100_000;
/** Teto de triângulos desenhados por hull (as janelas além disso são ignoradas). */
export const HULL_MAX_TRIANGLES = 400_000;
/** Teto de janelas por hull (barra loops se o backend nunca terminar). */
export const HULL_MAX_WINDOWS = 12;

export interface HullRequest {
  offset: number;
  limit: number;
}

/** O que `accept` precisa da resposta do backend (um `HullMesh` serve). */
export interface HullWindowInfo {
  offset: number;
  triangles: number;
  total_triangles: number;
  next_offset: number | null;
  truncated: boolean;
}

export interface HullProgress {
  /** triângulos recebidos */
  loaded: number;
  /** triângulos do hull inteiro (0 até chegar a primeira janela) */
  total: number;
  /** janelas recebidas */
  windows: number;
  /** janelas previstas para ler o hull inteiro dentro dos tetos */
  planned: number;
  /** terminou (acabou o hull, bateu num teto ou foi cancelado) */
  done: boolean;
  /** parou antes do fim por causa de um teto do cliente */
  capped: boolean;
  /** o backend avisou que o hull pode estar incompleto */
  truncated: boolean;
  /** 0..1 */
  fraction: number;
}

/** Janelas necessárias para cobrir `total` triângulos sem passar de `maxTriangles`/`maxWindows`. */
export function planHullWindows(
  total: number,
  windowSize = HULL_WINDOW,
  maxTriangles = HULL_MAX_TRIANGLES,
  maxWindows = HULL_MAX_WINDOWS,
): HullRequest[] {
  const out: HullRequest[] = [];
  const goal = Math.min(Math.max(0, total), maxTriangles);
  let offset = 0;
  while (offset < goal && out.length < maxWindows) {
    const limit = Math.min(windowSize, goal - offset);
    out.push({ offset, limit });
    offset += limit;
  }
  return out;
}

/**
 * Máquina de estados do carregamento: `next()` diz a próxima janela a pedir (ou `null` quando
 * acabou) e `accept()` registra a resposta. Trata `next_offset` que não avança (backend
 * incoerente) como fim, em vez de pedir a mesma janela para sempre.
 */
export class HullPager {
  private loaded = 0;
  private total = 0;
  private windows = 0;
  private cursor: number | null = 0;
  private finished = false;
  private capped = false;
  private truncated = false;

  constructor(
    private readonly windowSize = HULL_WINDOW,
    private readonly maxTriangles = HULL_MAX_TRIANGLES,
    private readonly maxWindows = HULL_MAX_WINDOWS,
  ) {}

  /** Próxima janela a pedir, ou `null` se terminou. */
  next(): HullRequest | null {
    if (this.finished || this.cursor === null) return null;
    if (this.windows >= this.maxWindows || this.loaded >= this.maxTriangles) {
      this.finished = true;
      this.capped = true;
      return null;
    }
    return { offset: this.cursor, limit: Math.min(this.windowSize, this.maxTriangles - this.loaded) };
  }

  /** Registra uma janela recebida e avança o cursor. */
  accept(w: HullWindowInfo): void {
    this.windows++;
    this.loaded += Math.max(0, w.triangles);
    this.total = Math.max(this.total, w.total_triangles);
    this.truncated ||= w.truncated;
    const advanced = w.next_offset !== null && w.next_offset > w.offset;
    if (w.next_offset === null || !advanced || w.triangles <= 0) {
      this.cursor = null;
      this.finished = true;
    } else {
      this.cursor = w.next_offset;
    }
  }

  /** Interrompe (o usuário desligou o hull ou fechou o viewer). */
  cancel(): void {
    this.finished = true;
    this.cursor = null;
  }

  progress(): HullProgress {
    const total = this.total;
    const goal = Math.min(total, this.maxTriangles);
    return {
      loaded: this.loaded,
      total,
      windows: this.windows,
      planned: total > 0 ? planHullWindows(total, this.windowSize, this.maxTriangles, this.maxWindows).length : 0,
      done: this.finished,
      capped: this.capped,
      truncated: this.truncated,
      fraction: this.finished && !this.capped ? 1 : goal > 0 ? Math.min(1, this.loaded / goal) : 0,
    };
  }
}

/** Converte os vértices de uma janela (mapa, Z-up) para o mundo do Three.js (Y-up). */
export function hullPositionsToWorld(positions: ArrayLike<number>): Float32Array {
  const out = new Float32Array(positions.length - (positions.length % 3));
  for (let i = 0; i + 2 < positions.length; i += 3) {
    out[i] = positions[i]!;
    out[i + 1] = positions[i + 2]!;
    out[i + 2] = -positions[i + 1]!;
  }
  return out;
}
