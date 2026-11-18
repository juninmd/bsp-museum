/** Release notes in pt-BR: grouped by Conventional Commit type, with a download table and the compare link. */
export type NoteCandidate = { sha: string; tag: string; version: string; previousTag: string; folded?: boolean };
export type Detail = { sha: string; subject: string };
type Entry = { type: string; scope: string; subject: string; breaking: boolean; sha?: string };

/** What each type becomes, in reading order; `docs`, `test`, `chore`, `ci`, `build` and `style` stay out of the notes. */
const GROUPS: { key: string; heading: string; one: string; many: string }[] = [
  { key: "breaking", heading: "⚠️ Mudanças incompatíveis", one: "mudança incompatível", many: "mudanças incompatíveis" },
  { key: "feat", heading: "✨ Novidades", one: "novidade", many: "novidades" },
  { key: "fix", heading: "🐛 Correções", one: "correção", many: "correções" },
  { key: "perf", heading: "⚡ Desempenho", one: "ganho de desempenho", many: "ganhos de desempenho" },
  { key: "refactor", heading: "♻️ Melhorias internas", one: "melhoria interna", many: "melhorias internas" },
];
const ICONS: Record<string, string> = { breaking: "⚠️", feat: "✨", fix: "🐛", perf: "⚡", refactor: "♻️" };
const MAX_ENTRIES = 40;
const MARKDOWN_SPECIALS = new Set("\\`*_{}[]()<>#+.!|~");

export function escapeMarkdown(text: string): string {
  return Array.from(text, (char) => (MARKDOWN_SPECIALS.has(char) ? `\\${char}` : char)).join("");
}

function firstTitle(message: string): string {
  const lines = message.split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
  return lines[0]?.startsWith("Merge pull request") ? lines[1] ?? lines[0] : lines[0] ?? "";
}

export function parseEntry(line: string, sha?: string): Entry | undefined {
  const match = /^([a-z][\w-]*)(?:\(([^)\r\n]+)\))?(!)?:\s+(.+)$/i.exec(line.trim());
  if (!match) return undefined;
  const subject = (match[4] ?? "").replace(/\s*\(#\d+\)\s*$/, "").trim();
  if (!subject) return undefined;
  return { type: (match[1] ?? "").toLowerCase(), scope: match[2]?.trim() ?? "", subject, breaking: Boolean(match[3]), sha };
}

/** The `#N` of a squash title (`fix: x (#58)`) or of a merge commit (`Merge pull request #61 from ...`). */
export function pullNumber(message: string): number | undefined {
  const first = message.split(/\r?\n/, 1)[0] ?? "";
  const found = /^Merge pull request #(\d+)\b/.exec(first) ?? /\(#(\d+)\)\s*$/.exec(first);
  return found ? Number(found[1]) : undefined;
}

/** Squash commits list their parts as `* type: subject` in the body. */
function bulletEntries(message: string): Entry[] {
  return message.split(/\r?\n/).slice(1).map((line) => /^[*-]\s+(.+)$/.exec(line.trim())?.[1]).flatMap((text) => {
    const entry = text ? parseEntry(text) : undefined;
    return entry ? [entry] : [];
  });
}

function entriesOf(message: string, details: Detail[]): Entry[] {
  const own = details.flatMap((d) => parseEntry(d.subject, d.sha) ?? []);
  const found = own.length > 0 ? own : bulletEntries(message);
  if (found.length > 0) return found;
  const title = parseEntry(firstTitle(message));
  return title ? [title] : [];
}

function unique(entries: Entry[]): Entry[] {
  const seen = new Set<string>();
  return entries.filter((e) => {
    const key = `${e.type}|${e.scope}|${e.subject.toLowerCase()}`;
    return seen.has(key) ? false : Boolean(seen.add(key));
  }).slice(0, MAX_ENTRIES);
}

function sentence(entry: Entry): string {
  const subject = entry.subject.charAt(0).toUpperCase() + entry.subject.slice(1);
  const scope = entry.scope ? `**${escapeMarkdown(entry.scope)}** · ` : "";
  return `${scope}${escapeMarkdown(subject)}`;
}

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

function downloads(root: string, tag: string, version: string): string {
  const file = (name: string, label = name) => `[${label}](${root}/releases/download/${tag}/${name})`;
  return [
    "| Sistema | Instalador |",
    "|---|---|",
    `| 🪟 Windows | ${file(`bsp-museum_${version}_x64-setup.exe`, "Instalador (.exe)")} · ${file(`bsp-museum_${version}_x64_en-US.msi`, "MSI")} |`,
    `| 🍎 macOS (Apple Silicon) | ${file(`bsp-museum_${version}_aarch64.dmg`, "Imagem de disco (.dmg)")} |`,
    `| 🍎 macOS (Intel) | ${file(`bsp-museum_${version}_x64.dmg`, "Imagem de disco (.dmg)")} |`,
    `| 🐧 Linux | ${file(`bsp-museum_${version}_amd64.AppImage`, "AppImage")} · ${file(`bsp-museum_${version}_amd64.deb`, ".deb")} · ${file(`bsp-museum-${version}-1.x86_64.rpm`, ".rpm")} |`,
  ].join("\n");
}

export function releaseNotes(message: string, candidate: NoteCandidate, repository: string, details: Detail[] = []): string {
  if (!/^[\w.-]+\/[\w.-]+$/.test(repository)) throw new Error("Repositório inválido");
  const title = firstTitle(message);
  if (!title) throw new Error("Commit sem título");
  const root = `https://github.com/${repository}`;
  const pull = pullNumber(message);
  const entries = unique(entriesOf(message, details));
  const grouped = new Map<string, Entry[]>();
  for (const entry of entries) {
    const key = entry.breaking || /^BREAKING[ -]CHANGE\s*:/im.test(message) && entries.length === 1 ? "breaking" : entry.type;
    if (ICONS[key]) grouped.set(key, [...(grouped.get(key) ?? []), entry]);
  }
  // A title that is not a Conventional Commit still deserves a line, instead of an empty release.
  if (grouped.size === 0) grouped.set("feat", [{ type: "feat", scope: "", subject: title.slice(0, 240), breaking: false }]);

  const link = (entry: Entry) => {
    if (entry.sha) return `[\`${entry.sha.slice(0, 7)}\`](${root}/commit/${entry.sha})`;
    return pull ? `[#${pull}](${root}/pull/${pull})` : `[\`${candidate.sha.slice(0, 7)}\`](${root}/commit/${candidate.sha})`;
  };
  const summary = GROUPS.flatMap((g) => (grouped.has(g.key) ? [`${ICONS[g.key]} ${plural(grouped.get(g.key)!.length, g.one, g.many)}`] : [])).join(" · ");
  const sections = GROUPS.flatMap((g) => {
    const list = grouped.get(g.key);
    if (!list) return [];
    return [`## ${g.heading}\n\n${list.map((e) => `- ${sentence(e)} (${link(e)})`).join("\n")}`];
  });
  // The title of a merged pull request sums up its commits listed below; a folded range has no such title.
  const headline = (entries.length > 1 || details.length > 0) && !candidate.folded ? parseEntry(title) : undefined;
  const intro = headline ? `> ${sentence(headline)}${pull ? ` ([#${pull}](${root}/pull/${pull}))` : ""}` : "";
  const compare = `${root}/compare/${candidate.previousTag}...${candidate.tag}`;
  return [
    summary,
    ...(intro ? [intro] : []),
    ...sections,
    `## ⬇️ Baixar\n\n${downloads(root, candidate.tag, candidate.version)}\n\nQuem já usa o bsp-museum recebe esta versão sozinho, pelo botão de atualização do app. Os instaladores são assinados e só entram se a assinatura confere.`,
    `**Todas as mudanças:** [${candidate.previousTag}…${candidate.tag}](${compare})`,
  ].join("\n\n") + "\n";
}
