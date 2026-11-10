/** Lógica pura da paleta de comandos (Ctrl/⌘+K): busca aproximada e ordenação. */

export interface PaletteItem {
  id: string;
  label: string;
  /** texto secundário (título do mapa, atalho…) */
  hint?: string;
  group: "action" | "map";
  /** termos extras que também casam (sinônimos, modo de jogo) */
  keywords?: string;
}

const strip = (s: string) => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();

/**
 * Pontua `query` contra `text`: -1 se as letras não aparecem em ordem. Maior = melhor.
 * Premia prefixo, início de palavra (`_`, `-`, espaço, `.`) e letras seguidas; penaliza o que sobra.
 */
export function fuzzyScore(query: string, text: string): number {
  const q = strip(query).replace(/\s+/g, "");
  const s = strip(text);
  if (!q) return 0;
  let score = 0;
  let pos = 0;
  let prev = -2;
  for (const ch of q) {
    const at = s.indexOf(ch, pos);
    if (at < 0) return -1;
    const boundary = at === 0 || /[\s_\-./]/.test(s[at - 1] ?? "");
    score += 1 + (at === prev + 1 ? 4 : 0) + (boundary ? 3 : 0) + (at === 0 ? 4 : 0);
    prev = at;
    pos = at + 1;
  }
  if (s.includes(strip(query).trim())) score += 10; // trecho contíguo
  return score - (s.length - q.length) * 0.05;
}

/** Melhores `limit` itens para `query`; sem consulta, a ordem original (ações primeiro). */
export function rankItems(items: PaletteItem[], query: string, limit = 40): PaletteItem[] {
  if (!query.trim()) return items.slice(0, limit);
  const scored: { item: PaletteItem; score: number }[] = [];
  for (const item of items) {
    const direct = fuzzyScore(query, item.label);
    const side = Math.max(fuzzyScore(query, item.hint ?? ""), fuzzyScore(query, item.keywords ?? "")) - 6;
    const score = Math.max(direct, side);
    if (score >= 0) scored.push({ item, score: score + (item.group === "action" ? 0.5 : 0) });
  }
  scored.sort((a, b) => b.score - a.score || a.item.label.localeCompare(b.item.label));
  return scored.slice(0, limit).map((s) => s.item);
}
