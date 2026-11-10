import { t } from "../i18n.ts";
import { rankItems, type PaletteItem } from "../lib/palette.ts";
import { escapeHtml, trapTab } from "./dom.ts";

export interface PaletteCommand extends PaletteItem {
  run: () => void | Promise<void>;
}

/**
 * Paleta de comandos (Ctrl/⌘+K, ou `/` fora de campos de texto): busca aproximada em
 * comandos e mapas, navegação por teclado. `source()` é chamado a cada abertura para
 * refletir o estado atual (mapas carregados, idioma, filtros).
 */
export function initPalette(source: () => PaletteCommand[], opener?: HTMLElement | null) {
  const root = document.createElement("div");
  root.className = "palette-scrim";
  root.hidden = true;
  root.innerHTML = `
    <div class="palette" role="dialog" aria-modal="true">
      <input class="palette-input" type="text" role="combobox" aria-expanded="true" aria-controls="palette-list" aria-autocomplete="list" autocomplete="off" spellcheck="false" />
      <ul class="palette-list" id="palette-list" role="listbox"></ul>
      <footer class="palette-foot"><span class="palette-hint"></span><span class="palette-count"></span></footer>
    </div>`;
  document.body.append(root);
  const dialog = root.querySelector<HTMLElement>(".palette")!;
  const input = root.querySelector<HTMLInputElement>(".palette-input")!;
  const list = root.querySelector<HTMLUListElement>(".palette-list")!;
  const count = root.querySelector<HTMLElement>(".palette-count")!;
  let all: PaletteCommand[] = [];
  let shown: PaletteCommand[] = [];
  let active = 0;
  let returnFocus: HTMLElement | null = null;

  const isOpen = () => !root.hidden;

  function draw() {
    const q = input.value;
    shown = rankItems(all, q, 50) as PaletteCommand[];
    active = Math.min(active, Math.max(shown.length - 1, 0));
    if (!shown.length) {
      list.innerHTML = `<li class="palette-empty" role="presentation">${escapeHtml(t("pal.empty", q.trim()))}</li>`;
    } else {
      let group = "";
      let html = "";
      shown.forEach((item, i) => {
        if (item.group !== group) {
          group = item.group;
          html += `<li class="palette-group" role="presentation">${escapeHtml(t(`pal.group.${group}`))}</li>`;
        }
        html += `<li id="pal-${i}" role="option" class="palette-item${i === active ? " active" : ""}" aria-selected="${i === active}" data-i="${i}">
          <span class="palette-label">${escapeHtml(item.label)}</span>${item.hint ? `<span class="palette-sub">${escapeHtml(item.hint)}</span>` : ""}</li>`;
      });
      list.innerHTML = html;
    }
    input.setAttribute("aria-activedescendant", shown.length ? `pal-${active}` : "");
    count.textContent = shown.length ? t("pal.count", shown.length) : "";
    list.querySelector(".active")?.scrollIntoView({ block: "nearest" });
  }

  function open(initial = "") {
    if (isOpen()) return;
    all = source();
    returnFocus = document.activeElement as HTMLElement | null;
    input.placeholder = t("pal.ph");
    dialog.setAttribute("aria-label", t("pal.aria"));
    root.querySelector<HTMLElement>(".palette-hint")!.textContent = t("pal.hint");
    input.value = initial;
    active = 0;
    root.hidden = false;
    draw();
    input.focus();
  }

  function close(restore = true) {
    if (!isOpen()) return;
    root.hidden = true;
    if (restore) returnFocus?.focus();
    returnFocus = null;
  }

  async function choose(i: number) {
    const item = shown[i];
    if (!item) return;
    close(false);
    await item.run();
  }

  input.addEventListener("input", () => {
    active = 0;
    draw();
  });
  input.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (!shown.length) return;
      active = (active + (e.key === "ArrowDown" ? 1 : -1) + shown.length) % shown.length;
      draw();
    } else if (e.key === "Home" || e.key === "End") {
      if (e.ctrlKey || !input.value) {
        e.preventDefault();
        active = e.key === "Home" ? 0 : Math.max(shown.length - 1, 0);
        draw();
      }
    } else if (e.key === "Enter") {
      e.preventDefault();
      void choose(active);
    }
  });
  list.addEventListener("mousemove", (e) => {
    const li = (e.target as HTMLElement).closest<HTMLElement>(".palette-item");
    if (!li) return;
    const i = Number(li.dataset.i);
    if (i !== active) {
      list.querySelector(".active")?.classList.remove("active");
      li.classList.add("active");
      active = i;
    }
  });
  list.addEventListener("click", (e) => {
    const li = (e.target as HTMLElement).closest<HTMLElement>(".palette-item");
    if (li) void choose(Number(li.dataset.i));
  });
  root.addEventListener("mousedown", (e) => {
    if (e.target === root) close();
  });
  dialog.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      close();
    } else trapTab(dialog, e);
  });

  const typing = (el: EventTarget | null) => {
    const n = el as HTMLElement | null;
    return !!n && (n.tagName === "INPUT" || n.tagName === "TEXTAREA" || n.tagName === "SELECT" || n.isContentEditable);
  };
  document.addEventListener("keydown", (e) => {
    if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === "k") {
      e.preventDefault();
      if (isOpen()) close();
      else open();
    } else if (e.key === "/" && !e.ctrlKey && !e.metaKey && !typing(e.target) && !isOpen()) {
      e.preventDefault();
      open();
    }
  });
  opener?.addEventListener("click", () => open());
  return { open, close, isOpen };
}
