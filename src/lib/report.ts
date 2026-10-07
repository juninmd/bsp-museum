import { findingText, getLang, modeLabel, t } from "../i18n.ts";
import type { AuditReport, AuditRow, ResourceReport, Severity } from "../types.ts";

export function fmtSize(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} B`;
}

const RANK: Record<Severity, number> = { critical: 0, warn: 1, info: 2 };

export interface AuditTotals {
  maps: number;
  critical: number;
  warn: number;
  info: number;
  clean: number;
  errors: number;
  duplicates: number;
}

export function auditTotals(report: AuditReport): AuditTotals {
  const totals: AuditTotals = { maps: report.rows.length, critical: 0, warn: 0, info: 0, clean: 0, errors: 0, duplicates: report.duplicates.length };
  for (const row of report.rows) {
    if (row.error) {
      totals.errors++;
      continue;
    }
    const worst = worstSeverity(row);
    if (worst === "critical") totals.critical++;
    else if (worst === "warn") totals.warn++;
    else if (worst === "info") totals.info++;
    else totals.clean++;
  }
  return totals;
}

export function worstSeverity(row: AuditRow): Severity | null {
  let worst: Severity | null = null;
  for (const f of row.findings) if (worst === null || RANK[f.severity] < RANK[worst]) worst = f.severity;
  return worst;
}

/** Linhas ordenadas: pior severidade primeiro, depois nome. */
export function sortedRows(report: AuditReport): AuditRow[] {
  const rank = (r: AuditRow) => (r.error ? -1 : (worstSeverity(r) ? RANK[worstSeverity(r)!] : 9));
  return [...report.rows].sort((a, b) => rank(a) - rank(b) || a.name.localeCompare(b.name));
}

function csvCell(value: string | number): string {
  let text = String(value);
  // nome de arquivo começando com = + - @ viraria fórmula ao abrir no Excel/Sheets
  if (/^[=+\-@\t\r]/.test(text)) text = `'${text}`;
  return /[",\n;]/.test(text) ? `"${text.replace(/"/g, '""')}"` : text;
}

export function auditCsv(report: AuditReport): string {
  const head = [t("report.col.map"), t("report.col.mode"), t("report.col.size"), t("report.col.spawns"), t("report.col.severity"), t("report.col.rule"), t("report.col.finding")];
  const lines = [head.join(",")];
  for (const row of sortedRows(report)) {
    const base = [row.name, modeLabel(row.mode), fmtSize(row.file_size), `${row.ct_spawns} CT / ${row.t_spawns} T`];
    if (row.error) lines.push([...base, "error", "erro-leitura", row.error].map(csvCell).join(","));
    else if (row.findings.length === 0) lines.push([...base, "ok", "", ""].map(csvCell).join(","));
    else for (const f of row.findings) lines.push([...base, f.severity, f.id, findingText(f).title].map(csvCell).join(","));
  }
  // BOM: sem ele o Excel abre o UTF-8 como ANSI e quebra os acentos
  return "\uFEFF" + lines.join("\n") + "\n";
}

const SEV_ICON: Record<Severity, string> = { critical: "✖", warn: "▲", info: "•" };

export function auditMarkdown(report: AuditReport): string {
  const totals = auditTotals(report);
  const out: string[] = [];
  out.push(`# ${t("report.title")}`, "");
  out.push(`- ${t("report.folder")}: \`${report.dir}\``);
  out.push(`- ${t("report.slots")}: ${report.slots}`);
  out.push(
    `- ${t("report.summary", totals.maps, totals.critical, totals.warn, totals.clean, totals.errors)}`,
    "",
  );
  out.push(`| ${t("report.col.map")} | ${t("report.col.mode")} | ${t("report.col.size")} | ${t("report.col.spawns")} | ${t("report.col.findings")} |`);
  out.push("|---|---|---|---|---|");
  for (const row of sortedRows(report)) {
    const findings = row.error
      ? `✖ ${row.error}`
      : row.findings.length === 0
        ? "✔"
        : row.findings.map((f) => `${SEV_ICON[f.severity]} ${findingText(f).title}`).join("<br>");
    out.push(`| \`${row.name}\` | ${modeLabel(row.mode)} | ${fmtSize(row.file_size)} | ${row.ct_spawns}/${row.t_spawns} | ${findings.replace(/\|/g, "\\|")} |`);
  }
  if (report.duplicates.length) {
    out.push("", `## ${t("report.duplicates")}`, "");
    for (const group of report.duplicates) out.push(`- ${group.map((p) => `\`${p}\``).join(" = ")}`);
  }
  return out.join("\n") + "\n";
}

const esc = (s: string) => s.replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c] ?? c);

export function auditHtml(report: AuditReport): string {
  const totals = auditTotals(report);
  const rows = sortedRows(report)
    .map((row) => {
      const cls = row.error ? "critical" : (worstSeverity(row) ?? "ok");
      const findings = row.error
        ? `<li class="critical">✖ ${esc(row.error)}</li>`
        : row.findings.length === 0
          ? `<li class="ok">✔</li>`
          : row.findings.map((f) => `<li class="${f.severity}">${SEV_ICON[f.severity]} ${esc(findingText(f).title)} <code>${esc(f.id)}</code></li>`).join("");
      return `<tr class="${cls}"><td><code>${esc(row.name)}</code></td><td>${esc(modeLabel(row.mode))}</td><td>${fmtSize(row.file_size)}</td><td>${row.ct_spawns}/${row.t_spawns}</td><td><ul>${findings}</ul></td></tr>`;
    })
    .join("\n");
  const dups = report.duplicates.length
    ? `<h2>${esc(t("report.duplicates"))}</h2><ul>${report.duplicates.map((g) => `<li>${g.map((p) => `<code>${esc(p)}</code>`).join(" = ")}</li>`).join("")}</ul>`
    : "";
  return `<!doctype html>
<html lang="${getLang()}"><head><meta charset="utf-8"><title>${esc(t("report.title"))}</title>
<style>
body{font:14px/1.5 system-ui,sans-serif;margin:2rem;color:#1b2430}
table{border-collapse:collapse;width:100%}th,td{border:1px solid #d0d7de;padding:.35rem .6rem;text-align:left;vertical-align:top}
th{background:#f3f5f7}ul{margin:0;padding-left:1.1rem;list-style:none}li.critical{color:#b42318}li.warn{color:#9a6700}li.ok{color:#1a7f37}
tr.critical td:first-child{border-left:4px solid #b42318}tr.warn td:first-child{border-left:4px solid #bf8700}code{font-size:.85em}
</style></head><body>
<h1>${esc(t("report.title"))}</h1>
<p>${esc(t("report.folder"))}: <code>${esc(report.dir)}</code> · ${esc(t("report.slots"))}: ${report.slots}</p>
<p>${esc(t("report.summary", totals.maps, totals.critical, totals.warn, totals.clean, totals.errors))}</p>
<table><thead><tr><th>${esc(t("report.col.map"))}</th><th>${esc(t("report.col.mode"))}</th><th>${esc(t("report.col.size"))}</th><th>${esc(t("report.col.spawns"))}</th><th>${esc(t("report.col.findings"))}</th></tr></thead>
<tbody>
${rows}
</tbody></table>
${dups}
</body></html>
`;
}

/** Lista de FastDL: um caminho por linha — só o que o jogador precisa baixar. */
export function fastdlText(report: ResourceReport): string {
  return report.items
    .filter((i) => i.found && !i.shared)
    .map((i) => i.path)
    .join("\n")
    .concat("\n");
}
