import { invoke } from "@tauri-apps/api/core";
import { fmtSize } from "../lib/report.ts";
import { modeLabel, ruleLabel, t } from "../i18n.ts";
import type { Comparison, CompareSide, CountDiff, MapSummary } from "../types.ts";
import { escapeHtml } from "./dom.ts";
import { openModal } from "./modal.ts";

function dims(side: CompareSide): string {
  const b = side.bounds;
  return b ? `${Math.round(b.size[0])} × ${Math.round(b.size[1])} × ${Math.round(b.size[2])}` : "—";
}

function delta(a: number, b: number, fmt: (n: number) => string = String): string {
  if (a === b) return `<span class="dim">=</span>`;
  const d = b - a;
  const cls = d > 0 ? "up" : "down";
  return `<span class="delta ${cls}">${d > 0 ? "+" : "−"}${escapeHtml(fmt(Math.abs(d)))}</span>`;
}

function factRow(label: string, a: string, b: string, extra = ""): string {
  const same = a === b;
  return `<tr class="${same ? "" : "diff"}"><th scope="row">${escapeHtml(label)}</th><td>${escapeHtml(a)}</td><td>${escapeHtml(b)}</td><td>${extra}</td></tr>`;
}

function diffTable(rows: CountDiff[], fmt: (n: number) => string = String, limit = 14): string {
  if (!rows.length) return `<p class="dim small">${escapeHtml(t("cmp.noDiff"))}</p>`;
  const body = rows
    .slice(0, limit)
    .map((r) => `<tr><td><code>${escapeHtml(r.name)}</code></td><td>${escapeHtml(fmt(r.a))}</td><td>${escapeHtml(fmt(r.b))}</td><td>${delta(r.a, r.b, fmt)}</td></tr>`)
    .join("");
  const more = rows.length > limit ? `<p class="dim small">${escapeHtml(t("ent.more", rows.length - limit))}</p>` : "";
  return `<table class="table cmp-table"><tbody>${body}</tbody></table>${more}`;
}

function chips(items: string[]): string {
  return items.length ? items.slice(0, 40).map((x) => `<code>${escapeHtml(x)}</code>`).join(" ") : `<span class="dim small">—</span>`;
}

export function renderComparison(c: Comparison): string {
  const idsA = new Set(c.a.findings.map(([id]) => id));
  const idsB = new Set(c.b.findings.map(([id]) => id));
  const fixed = [...idsA].filter((id) => !idsB.has(id));
  const added = [...idsB].filter((id) => !idsA.has(id));
  const rules = (ids: string[]) =>
    ids.length ? ids.map((id) => `<span class="pill">${escapeHtml(ruleLabel(id))}</span>`).join(" ") : `<span class="dim small">—</span>`;

  return `
    <table class="table cmp-table facts-table">
      <thead><tr><th></th><th><code>${escapeHtml(c.a.name)}</code></th><th><code>${escapeHtml(c.b.name)}</code></th><th>Δ</th></tr></thead>
      <tbody>
        ${factRow(t("cmp.mode"), modeLabel(c.a.mode), modeLabel(c.b.mode))}
        ${factRow(t("detail.file"), fmtSize(c.a.file_size), fmtSize(c.b.file_size), delta(c.a.file_size, c.b.file_size, fmtSize))}
        ${factRow(t("cmp.version"), `v${c.a.bsp_version}`, `v${c.b.bsp_version}`)}
        ${factRow(t("cmp.spawns"), `${c.a.ct_spawns} CT · ${c.a.t_spawns} T`, `${c.b.ct_spawns} CT · ${c.b.t_spawns} T`)}
        ${factRow(t("cmp.entities"), String(c.a.entities), String(c.b.entities), delta(c.a.entities, c.b.entities))}
        ${factRow(t("detail.faces"), String(c.a.faces), String(c.b.faces), delta(c.a.faces, c.b.faces))}
        ${factRow(t("detail.vertices"), String(c.a.vertices), String(c.b.vertices), delta(c.a.vertices, c.b.vertices))}
        ${factRow(t("detail.texTotal"), String(c.a.textures), String(c.b.textures), delta(c.a.textures, c.b.textures))}
        ${factRow(t("detail.dimensions"), dims(c.a), dims(c.b))}
        ${factRow("fullbright", c.a.fullbright ? t("common.yes") : t("common.no"), c.b.fullbright ? t("common.yes") : t("common.no"))}
      </tbody>
    </table>

    <div class="grid-2">
      <section><h3>${escapeHtml(t("cmp.fixed"))}</h3><p>${rules(fixed)}</p></section>
      <section><h3>${escapeHtml(t("cmp.added"))}</h3><p>${rules(added)}</p></section>
    </div>

    <section><h3>${escapeHtml(t("cmp.lumps"))}</h3>${diffTable(c.lumps, fmtSize)}</section>
    <section><h3>${escapeHtml(t("cmp.ents"))}</h3>${diffTable(c.entities)}</section>

    <div class="grid-2">
      <section><h3>${escapeHtml(t("cmp.texOnly", c.a.name))}</h3><p class="chips">${chips(c.textures_only_a)}</p></section>
      <section><h3>${escapeHtml(t("cmp.texOnly", c.b.name))}</h3><p class="chips">${chips(c.textures_only_b)}</p></section>
    </div>
    <div class="grid-2">
      <section><h3>${escapeHtml(t("cmp.wadOnly", c.a.name))}</h3><p class="chips">${chips(c.wads_only_a)}</p></section>
      <section><h3>${escapeHtml(t("cmp.wadOnly", c.b.name))}</h3><p class="chips">${chips(c.wads_only_b)}</p></section>
    </div>`;
}

export async function openCompare(a: MapSummary, b: MapSummary, slots: number): Promise<void> {
  const { body, alive } = openModal(t("cmp.title", a.name, b.name));
  body.innerHTML = `<div class="loading">${escapeHtml(t("common.loading"))}</div>`;
  try {
    const result = await invoke<Comparison>("compare_maps", { a: a.path, b: b.path, slots });
    if (!alive()) return;
    body.innerHTML = renderComparison(result);
  } catch (err) {
    if (!alive()) return;
    body.innerHTML = `<div class="loading error">${escapeHtml(String(err))}</div>`;
  }
}
