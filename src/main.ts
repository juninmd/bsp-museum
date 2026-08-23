import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { Finding, MapDetail, MapSummary, Settings } from "./types.ts";

const gallery = document.querySelector<HTMLElement>("#gallery")!;
const statusBar = document.querySelector<HTMLElement>("#status")!;
const search = document.querySelector<HTMLInputElement>("#search")!;
const modeFilter = document.querySelector<HTMLSelectElement>("#mode-filter")!;
const sortBy = document.querySelector<HTMLSelectElement>("#sort")!;
const pickButton = document.querySelector<HTMLButtonElement>("#pick")!;
const slotsInput = document.querySelector<HTMLInputElement>("#slots")!;
const drawer = document.querySelector<HTMLElement>("#drawer")!;
const drawerBody = document.querySelector<HTMLElement>("#drawer-body")!;
const scrim = document.querySelector<HTMLElement>("#scrim")!;

let maps: MapSummary[] = [];
let settings: Settings = { last_dir: null, slots: 32 };

const fmtSize = (bytes: number) =>
  bytes > 1024 * 1024 ? `${(bytes / 1024 / 1024).toFixed(1)} MB` : `${Math.round(bytes / 1024)} KB`;

const area = (m: MapSummary) => (m.bounds ? m.bounds.size[0] * m.bounds.size[1] : 0);
const spawns = (m: MapSummary) => m.ct_spawns + m.t_spawns;

/** Problemas visíveis já no resumo — o detalhe roda a lista completa. */
function quickProblems(m: MapSummary): string[] {
  const out: string[] = [];
  if (m.error) out.push(m.error);
  if (spawns(m) === 0) out.push("sem spawn");
  else if (m.ct_spawns === 0 || m.t_spawns === 0) out.push("spawn de um time só");
  if (m.fullbright) out.push("fullbright");
  if (
    m.mode !== "unknown" &&
    m.mode_by_entities !== "unknown" &&
    m.mode_by_entities !== "deathmatch" &&
    m.mode !== m.mode_by_entities
  ) {
    out.push("prefixo divergente");
  }
  if (m.mode === "bomb" && m.mode_by_entities !== "bomb") out.push("de_ sem alvo de bomba");
  if (m.mode === "hostage" && m.mode_by_entities !== "hostage") out.push("cs_ sem refém/resgate");
  return out;
}

function setStatus(text: string, tone: "info" | "error" = "info") {
  statusBar.textContent = text;
  statusBar.dataset.tone = tone;
}

function visibleMaps(): MapSummary[] {
  const q = search.value.trim().toLowerCase();
  const mode = modeFilter.value;
  let list = maps.filter((m) => {
    if (mode && m.mode !== mode) return false;
    if (!q) return true;
    return (
      m.name.toLowerCase().includes(q) ||
      (m.title ?? "").toLowerCase().includes(q) ||
      m.mode_label.toLowerCase().includes(q)
    );
  });

  const by = sortBy.value;
  list = [...list].sort((a, b) => {
    switch (by) {
      case "size":
        return b.file_size - a.file_size;
      case "spawns":
        return spawns(b) - spawns(a);
      case "area":
        return area(b) - area(a);
      case "problems":
        return quickProblems(b).length - quickProblems(a).length || a.name.localeCompare(b.name);
      default:
        return a.name.localeCompare(b.name);
    }
  });
  return list;
}

/** Miniaturas só são geradas quando o card entra na tela: 200 mapas de uma vez travaria. */
const lazyThumbs = new IntersectionObserver(
  (entries) => {
    for (const entry of entries) {
      if (!entry.isIntersecting) continue;
      const card = entry.target as HTMLElement;
      lazyThumbs.unobserve(card);
      const path = card.dataset.path;
      const holder = card.querySelector<HTMLElement>(".thumb");
      if (!path || !holder || holder.dataset.loaded) continue;
      holder.dataset.loaded = "1";
      invoke<string>("map_thumbnail", { path })
        .then((svg) => {
          holder.innerHTML = svg;
        })
        .catch((err) => {
          holder.classList.add("thumb-error");
          holder.textContent = String(err);
        });
    }
  },
  { rootMargin: "300px" },
);

function render() {
  const list = visibleMaps();
  gallery.replaceChildren();

  if (!maps.length) {
    gallery.innerHTML = `<p class="empty">Nenhum mapa carregado. Clique em <strong>escolher pasta…</strong> e aponte para a pasta <code>maps</code> do cstrike.</p>`;
    return;
  }
  if (!list.length) {
    gallery.innerHTML = `<p class="empty">Nenhum mapa bate com o filtro.</p>`;
    return;
  }

  for (const map of list) {
    const problems = quickProblems(map);
    const card = document.createElement("article");
    card.className = "card";
    card.dataset.path = map.path;
    if (problems.length) card.dataset.problem = "1";

    card.innerHTML = `
      <div class="thumb"><div class="spinner"></div></div>
      <div class="meta">
        <div class="row">
          <h2>${escapeHtml(map.name)}</h2>
          <span class="chip mode-${map.mode}">${escapeHtml(map.mode_label)}</span>
        </div>
        ${map.title ? `<p class="title">${escapeHtml(map.title)}</p>` : ""}
        <dl class="facts">
          <div><dt>spawns</dt><dd>${map.ct_spawns} CT · ${map.t_spawns} T</dd></div>
          <div><dt>entidades</dt><dd>${map.entity_count}</dd></div>
          <div><dt>arquivo</dt><dd>${fmtSize(map.file_size)}</dd></div>
        </dl>
        ${
          problems.length
            ? `<ul class="flags">${problems.map((p) => `<li>${escapeHtml(p)}</li>`).join("")}</ul>`
            : ""
        }
      </div>`;

    card.addEventListener("click", () => openDetail(map));
    gallery.append(card);
    lazyThumbs.observe(card);
  }

  const problems = list.filter((m) => quickProblems(m).length).length;
  setStatus(
    `${list.length} de ${maps.length} mapa(s)${problems ? ` · ${problems} com aviso` : ""}`,
  );
}

function escapeHtml(text: string): string {
  return text.replace(
    /[&<>"']/g,
    (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c] ?? c,
  );
}

const SEVERITY_ICON: Record<Finding["severity"], string> = {
  critical: "✖",
  warn: "▲",
  info: "•",
};

async function openDetail(map: MapSummary) {
  drawer.hidden = false;
  scrim.hidden = false;
  drawerBody.innerHTML = `<div class="loading">lendo ${escapeHtml(map.name)}…</div>`;

  try {
    const detail = await invoke<MapDetail>("map_detail", {
      path: map.path,
      slots: settings.slots,
    });
    drawerBody.innerHTML = detailHtml(detail);
    drawerBody.querySelector<HTMLButtonElement>("#export")?.addEventListener("click", async () => {
      const target = await save({
        defaultPath: `${detail.summary.name}.svg`,
        filters: [{ name: "SVG", extensions: ["svg"] }],
      });
      if (!target) return;
      await invoke("export_svg", { target, svg: detail.svg });
      setStatus(`planta exportada para ${target}`);
    });
  } catch (err) {
    drawerBody.innerHTML = `<div class="loading error">${escapeHtml(String(err))}</div>`;
  }
}

function detailHtml(d: MapDetail): string {
  const s = d.summary;
  const size = s.bounds
    ? `${Math.round(s.bounds.size[0])} × ${Math.round(s.bounds.size[1])} × ${Math.round(s.bounds.size[2])} un`
    : "—";

  const findings = d.findings.length
    ? d.findings
        .map(
          (f) => `
      <li class="finding ${f.severity}">
        <div class="finding-head">${SEVERITY_ICON[f.severity]} ${escapeHtml(f.title)} <code>${escapeHtml(f.id)}</code></div>
        <p>${escapeHtml(f.detail)}</p>
        <p class="hint">→ ${escapeHtml(f.hint)}</p>
      </li>`,
        )
        .join("")
    : `<li class="finding ok">✔ Nenhum problema conhecido</li>`;

  const topEntities = d.histogram
    .slice(0, 14)
    .map(([name, count]) => `<tr><td><code>${escapeHtml(name)}</code></td><td>${count}</td></tr>`)
    .join("");

  const lumps = d.lumps
    .filter((l) => l.length > 0)
    .slice(0, 8)
    .map(
      (l) => `<tr>
        <td>${escapeHtml(l.name)}</td>
        <td>${fmtSize(l.length)}</td>
        <td><div class="bar" style="width:${Math.max(2, l.percent).toFixed(1)}%"></div>${l.percent.toFixed(1)}%</td>
      </tr>`,
    )
    .join("");

  return `
    <header class="detail-head">
      <h2>${escapeHtml(s.name)}</h2>
      <span class="chip mode-${s.mode}">${escapeHtml(s.mode_label)}</span>
      <button id="export" class="ghost">exportar planta (SVG)</button>
    </header>
    ${s.title ? `<p class="title">“${escapeHtml(s.title)}”</p>` : ""}

    <div class="plan">${d.svg}</div>
    <p class="legend">
      <span class="key ct"></span> spawn CT
      <span class="key t"></span> spawn T
      <span class="key grad"></span> altura (baixo → alto)
      <span class="dim">${d.polygons} polígonos desenhados</span>
    </p>

    <section>
      <h3>Diagnóstico</h3>
      <ul class="findings">${findings}</ul>
    </section>

    <section class="grid-2">
      <div>
        <h3>Geometria</h3>
        <dl class="facts col">
          <div><dt>dimensões</dt><dd>${size}</dd></div>
          <div><dt>faces</dt><dd>${d.face_count.toLocaleString("pt-BR")}</dd></div>
          <div><dt>vértices</dt><dd>${d.vertex_count.toLocaleString("pt-BR")}</dd></div>
          <div><dt>modelos (brush entities)</dt><dd>${d.model_count}</dd></div>
          <div><dt>arquivo</dt><dd>${fmtSize(s.file_size)}</dd></div>
        </dl>
      </div>
      <div>
        <h3>Texturas</h3>
        <dl class="facts col">
          <div><dt>total</dt><dd>${d.texture_count}</dd></div>
          <div><dt>embutidas no BSP</dt><dd>${d.embedded_textures}</dd></div>
          <div><dt>céu</dt><dd>${d.sky ? escapeHtml(d.sky) : "—"}</dd></div>
          <div><dt>WADs declarados</dt><dd>${d.wads.length ? escapeHtml(d.wads.join(", ")) : "nenhum"}</dd></div>
        </dl>
      </div>
    </section>

    <section class="grid-2">
      <div>
        <h3>Entidades mais usadas</h3>
        <table class="table"><tbody>${topEntities}</tbody></table>
      </div>
      <div>
        <h3>O que pesa no arquivo</h3>
        <table class="table lumps"><tbody>${lumps}</tbody></table>
      </div>
    </section>

    <details class="texlist">
      <summary>${d.textures.length} textura(s) usadas</summary>
      <p>${d.textures.map((t) => `<code>${escapeHtml(t)}</code>`).join(" ")}</p>
    </details>`;
}

function closeDrawer() {
  drawer.hidden = true;
  scrim.hidden = true;
  drawerBody.replaceChildren();
}

async function loadDir(dir: string) {
  setStatus(`lendo ${dir}…`);
  try {
    maps = await invoke<MapSummary[]>("scan_maps", { dir });
    settings.last_dir = dir;
    await invoke("save_settings", { settings });
    fillModeFilter();
    render();
  } catch (err) {
    setStatus(String(err), "error");
  }
}

function fillModeFilter() {
  const seen = new Map<string, string>();
  for (const m of maps) seen.set(m.mode, m.mode_label);
  const current = modeFilter.value;
  modeFilter.replaceChildren(new Option("todos os modos", ""));
  for (const [value, label] of [...seen].sort((a, b) => a[1].localeCompare(b[1]))) {
    modeFilter.append(new Option(label, value));
  }
  modeFilter.value = current;
}

pickButton.addEventListener("click", async () => {
  const dir = await open({ directory: true, multiple: false, title: "pasta com os .bsp" });
  if (typeof dir === "string") await loadDir(dir);
});

slotsInput.addEventListener("change", async () => {
  const value = Number.parseInt(slotsInput.value, 10);
  settings.slots = Number.isFinite(value) ? Math.min(128, Math.max(2, value)) : 32;
  slotsInput.value = String(settings.slots);
  await invoke("save_settings", { settings });
  render();
});

search.addEventListener("input", render);
modeFilter.addEventListener("change", render);
sortBy.addEventListener("change", render);
document.querySelector("#close-drawer")?.addEventListener("click", closeDrawer);
scrim.addEventListener("click", closeDrawer);
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape") closeDrawer();
});

(async () => {
  settings = await invoke<Settings>("load_settings");
  slotsInput.value = String(settings.slots);
  if (settings.last_dir) await loadDir(settings.last_dir);
  else render();
})();
