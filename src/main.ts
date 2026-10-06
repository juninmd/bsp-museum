import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { mountModelViewer } from "./resources.ts";
import { applyStatic, detectLang, getLang, modeLabel, ruleLabel, setLang, t } from "./i18n.ts";
import {
  allRules,
  allTags,
  EMPTY_FILTERS,
  filterAndSort,
  flagsOf,
  type FilterState,
  type SeverityFilter,
  type SortKey,
} from "./lib/filters.ts";
import { fmtSize } from "./lib/report.ts";
import { annotationOf, applySettings, maps, saveSettings, setMaps, settings, updateAnnotation } from "./store.ts";
import type { Lang, MapDetail, MapSummary, MdlSummary, ModelDir, Settings, Theme } from "./types.ts";
import { openAudit } from "./ui/audit.ts";
import { openCompare } from "./ui/compare.ts";
import { detailHtml, disposeDetail, wireDetail } from "./ui/detail.ts";
import { escapeHtml, trapTab } from "./ui/dom.ts";
import { initModal, isModalOpen } from "./ui/modal.ts";

const gallery = document.querySelector<HTMLElement>("#gallery")!;
const statusBar = document.querySelector<HTMLElement>("#status")!;
const search = document.querySelector<HTMLInputElement>("#search")!;
const modeFilter = document.querySelector<HTMLSelectElement>("#mode-filter")!;
const sevFilter = document.querySelector<HTMLSelectElement>("#sev-filter")!;
const ruleFilter = document.querySelector<HTMLSelectElement>("#rule-filter")!;
const tagFilter = document.querySelector<HTMLSelectElement>("#tag-filter")!;
const favOnly = document.querySelector<HTMLButtonElement>("#fav-only")!;
const sortBy = document.querySelector<HTMLSelectElement>("#sort")!;
const pickButton = document.querySelector<HTMLButtonElement>("#pick")!;
const compareBtn = document.querySelector<HTMLButtonElement>("#compare")!;
const auditBtn = document.querySelector<HTMLButtonElement>("#audit")!;
const langBtn = document.querySelector<HTMLButtonElement>("#lang-toggle")!;
const themeBtn = document.querySelector<HTMLButtonElement>("#theme-toggle")!;
const slotsInput = document.querySelector<HTMLInputElement>("#slots")!;
const drawer = document.querySelector<HTMLElement>("#drawer")!;
const drawerBody = document.querySelector<HTMLElement>("#drawer-body")!;
const scrim = document.querySelector<HTMLElement>("#scrim")!;
const closeDrawerBtn = document.querySelector<HTMLButtonElement>("#close-drawer")!;

const navMaps = document.querySelector<HTMLButtonElement>("#nav-maps")!;
const navResources = document.querySelector<HTMLButtonElement>("#nav-resources")!;
const mapControls = document.querySelector<HTMLElement>("#map-controls")!;
const resourcesSection = document.querySelector<HTMLElement>("#resources")!;
const resPickBtn = document.querySelector<HTMLButtonElement>("#res-pick")!;
const resRootLabel = document.querySelector<HTMLElement>("#res-root")!;
const resDirs = document.querySelector<HTMLElement>("#res-dirs")!;
const resFiles = document.querySelector<HTMLElement>("#res-files")!;
const resViewer3d = document.querySelector<HTMLElement>("#res-viewer3d")!;
const resViewerToolbar = document.querySelector<HTMLElement>("#res-viewer-toolbar")!;

/** elemento a devolver o foco quando o drawer fechar (o card que foi clicado/ativado) */
let lastFocused: HTMLElement | null = null;

/** filtros da galeria (a UI de seleção é só um espelho deste objeto) */
const filters: FilterState = { ...EMPTY_FILTERS };
/** mapas marcados para comparar (no máximo 2, por caminho) */
const compareSet: string[] = [];
/** detalhe aberto no momento (para redesenhar ao trocar de idioma) */
let current: { map: MapSummary; detail: MapDetail } | null = null;

function setStatus(text: string, tone: "info" | "error" = "info") {
  statusBar.textContent = text;
  statusBar.dataset.tone = tone;
}

// ------------------------------------------------------------------ tema e idioma

function systemTheme(): Theme {
  return window.matchMedia?.("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
  themeBtn.textContent = theme === "dark" ? "☾" : "☀";
  themeBtn.setAttribute("aria-label", t("tools.theme"));
}

function refreshLanguage() {
  applyStatic();
  langBtn.textContent = getLang().toUpperCase();
  fillModeFilter();
  fillRuleFilter();
  fillTagFilter();
  applyTheme((document.documentElement.dataset.theme as Theme) || "dark");
  favOnly.title = t("filter.fav");
  render();
  updateCompareButton();
  if (current && !drawer.hidden) renderCurrentDetail();
}

// ------------------------------------------------------------------ galeria

function visibleMaps(): MapSummary[] {
  return filterAndSort(maps, filters, settings.annotations, settings.slots, (m) => modeLabel(m.mode));
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

function updateCompareButton() {
  compareBtn.disabled = compareSet.length !== 2;
  compareBtn.textContent = compareSet.length ? t("compare.buttonN", compareSet.length) : t("compare.button");
}

function toggleCompare(map: MapSummary, on: boolean) {
  const at = compareSet.indexOf(map.path);
  if (on && at < 0) {
    if (compareSet.length === 2) compareSet.shift(); // o mais antigo sai
    compareSet.push(map.path);
  } else if (!on && at >= 0) {
    compareSet.splice(at, 1);
  }
  updateCompareButton();
  for (const box of gallery.querySelectorAll<HTMLInputElement>(".cmp-check")) {
    box.checked = compareSet.includes(box.closest<HTMLElement>(".card")!.dataset.path!);
  }
}

function render() {
  const list = visibleMaps();
  gallery.replaceChildren();

  if (!maps.length) {
    gallery.innerHTML = `<p class="empty">${t("gallery.empty")}</p>`;
    return;
  }
  if (!list.length) {
    gallery.innerHTML = `<p class="empty">${escapeHtml(t("gallery.noMatch"))}</p>`;
    return;
  }

  for (const map of list) {
    const flags = flagsOf(map, settings.slots);
    const note = annotationOf(map);
    const card = document.createElement("article");
    card.className = "card";
    card.dataset.path = map.path;
    if (flags.length) card.dataset.problem = flags.some((f) => f.severity === "critical") ? "critical" : "warn";

    card.innerHTML = `
      <div class="card-open" role="button" tabindex="0" aria-label="${escapeHtml(t("card.open", map.name))}">
        <div class="thumb"><div class="spinner"></div></div>
        <div class="meta">
          <div class="row">
            <h2>${escapeHtml(map.name)}</h2>
            <span class="chip mode-${map.mode}">${escapeHtml(modeLabel(map.mode))}</span>
          </div>
          ${map.title ? `<p class="title">${escapeHtml(map.title)}</p>` : ""}
          <dl class="facts">
            <div><dt>${escapeHtml(t("card.spawns"))}</dt><dd>${map.ct_spawns} CT · ${map.t_spawns} T</dd></div>
            <div><dt>${escapeHtml(t("card.entities"))}</dt><dd>${map.entity_count}</dd></div>
            <div><dt>${escapeHtml(t("card.file"))}</dt><dd>${fmtSize(map.file_size)}</dd></div>
          </dl>
          ${
            flags.length
              ? `<ul class="flags">${flags.map((p) => `<li class="${p.severity}">${escapeHtml(ruleLabel(p.id))}</li>`).join("")}</ul>`
              : ""
          }
          ${note.tags.length ? `<ul class="card-tags">${note.tags.map((tag) => `<li>${escapeHtml(tag)}</li>`).join("")}</ul>` : ""}
        </div>
      </div>
      <div class="card-tools">
        <label class="cmp-label" title="${escapeHtml(t("card.compare"))}">
          <input type="checkbox" class="cmp-check" aria-label="${escapeHtml(t("card.compareAria", map.name))}" ${compareSet.includes(map.path) ? "checked" : ""} />
        </label>
        <button class="fav-btn" aria-pressed="${note.favorite}" aria-label="${escapeHtml(t("note.favorite"))}" title="${escapeHtml(t("note.favorite"))}">${note.favorite ? "★" : "☆"}</button>
      </div>`;

    const opener = card.querySelector<HTMLElement>(".card-open")!;
    opener.addEventListener("click", () => openDetail(map, opener));
    opener.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        openDetail(map, opener);
      }
    });
    card.querySelector<HTMLInputElement>(".cmp-check")!.addEventListener("change", (e) =>
      toggleCompare(map, (e.target as HTMLInputElement).checked),
    );
    card.querySelector<HTMLButtonElement>(".fav-btn")!.addEventListener("click", async () => {
      await updateAnnotation(map, { favorite: !annotationOf(map).favorite });
      render();
      fillTagFilter();
    });
    gallery.append(card);
    lazyThumbs.observe(card);
  }

  const problems = list.filter((m) => flagsOf(m, settings.slots).length).length;
  setStatus(
    problems ? t("status.count.problems", list.length, maps.length, problems) : t("status.count", list.length, maps.length),
  );
}

// ------------------------------------------------------------------ detalhe

async function openDetail(map: MapSummary, trigger?: HTMLElement) {
  lastFocused = trigger ?? (document.activeElement as HTMLElement | null);
  drawer.hidden = false;
  scrim.hidden = false;
  drawerBody.innerHTML = `<div class="loading">${escapeHtml(t("detail.reading", map.name))}</div>`;
  document.addEventListener("keydown", onDrawerKeydown);
  closeDrawerBtn.focus();

  try {
    const detail = await invoke<MapDetail>("map_detail", { path: map.path, slots: settings.slots });
    current = { map, detail };
    renderCurrentDetail();
  } catch (err) {
    drawerBody.innerHTML = `<div class="loading error">${escapeHtml(String(err))}</div>`;
  }
}

function renderCurrentDetail() {
  if (!current) return;
  disposeDetail();
  drawerBody.innerHTML = detailHtml(current.detail, current.map);
  wireDetail(drawerBody, current.detail, current.map, {
    slots: settings.slots,
    setStatus,
    onAnnotationChange: () => {
      render();
      fillTagFilter();
    },
  });
}

/** mantém o Tab preso dentro do drawer enquanto ele estiver aberto (foco não vaza pro fundo) */
function onDrawerKeydown(e: KeyboardEvent) {
  if (isModalOpen()) return;
  trapTab(drawer, e);
}

function closeDrawer() {
  disposeDetail();
  current = null;
  drawer.hidden = true;
  scrim.hidden = true;
  drawerBody.replaceChildren();
  document.removeEventListener("keydown", onDrawerKeydown);
  lastFocused?.focus();
  lastFocused = null;
}

// ------------------------------------------------------------------ pasta e filtros

async function loadDir(dir: string) {
  setStatus(t("status.reading", dir));
  try {
    setMaps(await invoke<MapSummary[]>("scan_maps", { dir }));
    settings.last_dir = dir;
    await saveSettings();
    compareSet.length = 0;
    updateCompareButton();
    fillModeFilter();
    fillRuleFilter();
    fillTagFilter();
    render();
  } catch (err) {
    setStatus(String(err), "error");
  }
}

function fillSelect(select: HTMLSelectElement, first: [string, string], options: [string, string][]) {
  const current = select.value;
  select.replaceChildren(new Option(first[1], first[0]));
  for (const [value, label] of options) select.append(new Option(label, value));
  select.value = options.some(([v]) => v === current) ? current : "";
}

function fillModeFilter() {
  const seen = new Map<string, string>();
  for (const m of maps) seen.set(m.mode, modeLabel(m.mode));
  fillSelect(modeFilter, ["", t("filter.mode.all")], [...seen].sort((a, b) => a[1].localeCompare(b[1])));
  filters.mode = modeFilter.value;
}

function fillRuleFilter() {
  const rules = allRules(maps, settings.slots).map((id): [string, string] => [id, ruleLabel(id)]);
  fillSelect(ruleFilter, ["", t("filter.rule.any")], rules.sort((a, b) => a[1].localeCompare(b[1])));
  filters.rule = ruleFilter.value;
}

function fillTagFilter() {
  const tags = allTags(settings.annotations).map((tag): [string, string] => [tag, tag]);
  fillSelect(tagFilter, ["", t("filter.tag.any")], tags);
  filters.tag = tagFilter.value;
}

// -------------------------------------------------------------- Recursos (aba avulsa)

let resViewer: { dispose(): void } | null = null;
let resRoot = "";

function switchView(view: "maps" | "resources") {
  const showMaps = view === "maps";
  navMaps.classList.toggle("active", showMaps);
  navMaps.setAttribute("aria-pressed", String(showMaps));
  navResources.classList.toggle("active", !showMaps);
  navResources.setAttribute("aria-pressed", String(!showMaps));
  mapControls.hidden = !showMaps;
  gallery.hidden = !showMaps;
  statusBar.hidden = !showMaps;
  resourcesSection.hidden = showMaps;
  if (showMaps) {
    resViewer?.dispose();
    resViewer = null;
  }
}

function renderResDirs(dirs: ModelDir[]) {
  resDirs.replaceChildren();
  if (!dirs.length) {
    resDirs.innerHTML = `<p class="empty">${escapeHtml(t("mdl.noneFound"))}</p>`;
    return;
  }
  for (const dir of dirs) {
    const item = document.createElement("button");
    item.className = "res-item";
    item.textContent = `${dir.name} (${dir.count})`;
    item.addEventListener("click", async () => {
      resDirs.querySelectorAll(".res-item").forEach((el) => el.classList.remove("active"));
      item.classList.add("active");
      const files = await invoke<string[]>("list_models", { dir: dir.path });
      renderResFiles(files);
    });
    resDirs.append(item);
  }
}

function renderResFiles(files: string[]) {
  resFiles.replaceChildren();
  if (!files.length) {
    resFiles.innerHTML = `<p class="empty">${escapeHtml(t("mdl.emptyDir"))}</p>`;
    return;
  }
  for (const path of files) {
    const name = path.split(/[\\/]/).pop() ?? path;
    const item = document.createElement("button");
    item.className = "res-item";
    item.textContent = name;
    item.addEventListener("click", async () => {
      resFiles.querySelectorAll(".res-item").forEach((el) => el.classList.remove("active"));
      item.classList.add("active");
      await openModel(path);
    });
    resFiles.append(item);
  }
}

async function openModel(path: string) {
  resViewer?.dispose();
  resViewer = null;
  resViewerToolbar.hidden = true;
  resViewer3d.innerHTML = `<div class="loading">${escapeHtml(t("mdl.decoding"))}</div>`;
  try {
    const model = await invoke<MdlSummary>("load_model", { path });
    resViewer3d.replaceChildren();
    resViewer = mountModelViewer(resViewer3d, model, path);
    resViewerToolbar.hidden = model.sequences.length === 0;
  } catch (err) {
    resViewer3d.innerHTML = `<div class="loading error">${escapeHtml(String(err))}</div>`;
  }
}

navMaps.addEventListener("click", () => switchView("maps"));
navResources.addEventListener("click", () => switchView("resources"));

resPickBtn.addEventListener("click", async () => {
  const dir = await open({ directory: true, multiple: false, title: t("mdl.pickTitle") });
  if (typeof dir !== "string") return;
  resRoot = dir;
  resRootLabel.textContent = dir;
  resFiles.replaceChildren();
  resViewer?.dispose();
  resViewer = null;
  resViewer3d.replaceChildren();
  resViewerToolbar.hidden = true;
  try {
    const dirs = await invoke<ModelDir[]>("list_model_dirs", { root: resRoot });
    renderResDirs(dirs);
  } catch (err) {
    resDirs.innerHTML = `<div class="loading error">${escapeHtml(String(err))}</div>`;
  }
});

// ------------------------------------------------------------------------------------

pickButton.addEventListener("click", async () => {
  const dir = await open({ directory: true, multiple: false, title: t("pick.title") });
  if (typeof dir === "string") await loadDir(dir);
});

slotsInput.addEventListener("change", async () => {
  const value = Number.parseInt(slotsInput.value, 10);
  settings.slots = Number.isFinite(value) ? Math.min(128, Math.max(2, value)) : 32;
  slotsInput.value = String(settings.slots);
  await saveSettings();
  fillRuleFilter();
  render();
});

let searchDebounce = 0;
search.addEventListener("input", () => {
  window.clearTimeout(searchDebounce);
  searchDebounce = window.setTimeout(() => {
    filters.query = search.value;
    render();
  }, 120);
});
modeFilter.addEventListener("change", () => {
  filters.mode = modeFilter.value;
  render();
});
sevFilter.addEventListener("change", () => {
  filters.severity = sevFilter.value as SeverityFilter;
  render();
});
ruleFilter.addEventListener("change", () => {
  filters.rule = ruleFilter.value;
  render();
});
tagFilter.addEventListener("change", () => {
  filters.tag = tagFilter.value;
  render();
});
sortBy.addEventListener("change", () => {
  filters.sort = sortBy.value as SortKey;
  render();
});
favOnly.addEventListener("click", () => {
  filters.favoritesOnly = !filters.favoritesOnly;
  favOnly.setAttribute("aria-pressed", String(filters.favoritesOnly));
  favOnly.textContent = filters.favoritesOnly ? "★" : "☆";
  render();
});

compareBtn.addEventListener("click", () => {
  const [a, b] = compareSet.map((p) => maps.find((m) => m.path === p));
  if (a && b) void openCompare(a, b, settings.slots);
});

auditBtn.addEventListener("click", () => {
  if (!settings.last_dir) {
    setStatus(t("audit.needFolder"), "error");
    return;
  }
  void openAudit(
    settings.last_dir,
    settings.slots,
    (path) => {
      const map = maps.find((m) => m.path === path);
      if (map) void openDetail(map);
    },
    setStatus,
  );
});

langBtn.addEventListener("click", async () => {
  const next: Lang = getLang() === "pt" ? "en" : "pt";
  settings.lang = next;
  setLang(next);
  refreshLanguage();
  await saveSettings();
});

themeBtn.addEventListener("click", async () => {
  const next: Theme = document.documentElement.dataset.theme === "light" ? "dark" : "light";
  settings.theme = next;
  applyTheme(next);
  await saveSettings();
});

closeDrawerBtn.addEventListener("click", closeDrawer);
scrim.addEventListener("click", closeDrawer);
initModal();
document.addEventListener("keydown", (e) => {
  if (e.key === "Escape" && !isModalOpen() && !drawer.hidden) closeDrawer();
});

void (async () => {
  try {
    applySettings(await invoke<Partial<Settings>>("load_settings"));
  } catch {
    // sem backend: segue com os padrões
  }
  setLang(detectLang(settings.lang, navigator.language));
  applyTheme(settings.theme ?? systemTheme());
  slotsInput.value = String(settings.slots);
  refreshLanguage();
  if (settings.last_dir) await loadDir(settings.last_dir);
  else render();
})();
