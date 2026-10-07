import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import { auditCsv, auditHtml, auditMarkdown, auditTotals, fmtSize, sortedRows, worstSeverity } from "../lib/report.ts";
import { findingText, modeLabel, t } from "../i18n.ts";
import type { AuditReport } from "../types.ts";
import { escapeHtml } from "./dom.ts";
import { openModal, closeModal } from "./modal.ts";

const ICON = { critical: "✖", warn: "▲", info: "•" } as const;

export function renderAudit(report: AuditReport): string {
  const totals = auditTotals(report);
  const rows = sortedRows(report)
    .map((row) => {
      const worst = row.error ? "critical" : (worstSeverity(row) ?? "ok");
      const findings = row.error
        ? `<li class="critical">${ICON.critical} ${escapeHtml(row.error)}</li>`
        : row.findings.length === 0
          ? `<li class="ok">✔</li>`
          : row.findings.map((f) => `<li class="${f.severity}">${ICON[f.severity]} ${escapeHtml(findingText(f).title)}</li>`).join("");
      return `<tr class="audit-${worst}" data-path="${escapeHtml(row.path)}">
        <td><button class="link audit-open" data-path="${escapeHtml(row.path)}"><code>${escapeHtml(row.name)}</code></button></td>
        <td>${escapeHtml(modeLabel(row.mode))}</td><td>${fmtSize(row.file_size)}</td>
        <td>${row.ct_spawns}/${row.t_spawns}</td><td><ul class="audit-findings">${findings}</ul></td></tr>`;
    })
    .join("");
  const dups = report.duplicates.length
    ? `<section><h3>${escapeHtml(t("report.duplicates"))}</h3><ul>${report.duplicates
        .map((g) => `<li>${g.map((p) => `<code>${escapeHtml(p.split(/[\\/]/).pop() ?? p)}</code>`).join(" = ")}</li>`)
        .join("")}</ul></section>`
    : "";
  return `
    <div class="audit-totals">
      <span class="stat"><strong>${totals.maps}</strong> ${escapeHtml(t("audit.maps"))}</span>
      <span class="stat crit"><strong>${totals.critical}</strong> ${escapeHtml(t("audit.critical"))}</span>
      <span class="stat warn"><strong>${totals.warn}</strong> ${escapeHtml(t("audit.warn"))}</span>
      <span class="stat ok"><strong>${totals.clean}</strong> ${escapeHtml(t("audit.clean"))}</span>
      <span class="stat"><strong>${totals.errors}</strong> ${escapeHtml(t("audit.errors"))}</span>
      <span class="stat"><strong>${totals.duplicates}</strong> ${escapeHtml(t("audit.dups"))}</span>
    </div>
    <div class="audit-actions">
      <button class="ghost2" data-fmt="md">${escapeHtml(t("audit.exportMd"))}</button>
      <button class="ghost2" data-fmt="csv">${escapeHtml(t("audit.exportCsv"))}</button>
      <button class="ghost2" data-fmt="html">${escapeHtml(t("audit.exportHtml"))}</button>
    </div>
    <div class="table-wrap"><table class="table audit-table">
      <thead><tr><th>${escapeHtml(t("report.col.map"))}</th><th>${escapeHtml(t("report.col.mode"))}</th><th>${escapeHtml(t("report.col.size"))}</th><th>${escapeHtml(t("report.col.spawns"))}</th><th>${escapeHtml(t("report.col.findings"))}</th></tr></thead>
      <tbody>${rows}</tbody>
    </table></div>
    ${dups}`;
}

const FORMATS = {
  md: { ext: "md", name: "Markdown", make: auditMarkdown },
  csv: { ext: "csv", name: "CSV", make: auditCsv },
  html: { ext: "html", name: "HTML", make: auditHtml },
} as const;

/** Roda a auditoria da pasta e mostra o relatório; `onOpen` abre o detalhe de um mapa. */
export async function openAudit(
  dir: string,
  slots: number,
  onOpen: (path: string) => void,
  setStatus: (text: string, tone?: "info" | "error") => void,
): Promise<void> {
  const { body, alive } = openModal(t("audit.title2"));
  body.innerHTML = `<div class="loading">${escapeHtml(t("audit.running"))}</div>`;
  let report: AuditReport;
  try {
    report = await invoke<AuditReport>("audit_folder", { dir, slots });
  } catch (err) {
    if (alive()) body.innerHTML = `<div class="loading error">${escapeHtml(String(err))}</div>`;
    return;
  }
  if (!alive()) return;
  body.innerHTML = renderAudit(report);
  body.querySelectorAll<HTMLButtonElement>("[data-fmt]").forEach((btn) =>
    btn.addEventListener("click", async () => {
      const f = FORMATS[btn.dataset.fmt as keyof typeof FORMATS];
      const target = await save({ defaultPath: `audit.${f.ext}`, filters: [{ name: f.name, extensions: [f.ext] }] });
      if (!target) return;
      await invoke("export_text", { target, content: f.make(report) });
      setStatus(t("status.exported", target));
    }),
  );
  body.querySelectorAll<HTMLButtonElement>(".audit-open").forEach((btn) =>
    btn.addEventListener("click", () => {
      closeModal();
      onOpen(btn.dataset.path!);
    }),
  );
}
