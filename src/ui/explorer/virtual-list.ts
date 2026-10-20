// Lista virtual: só as linhas visíveis existem no DOM (a listagem de um mod passa de 10 mil
// arquivos). Altura de linha fixa; teclado (setas, Home/End, Enter) e `role="listbox"`.

import { visibleRange } from "../../lib/explorer.ts";

export interface VirtualListOptions<T> {
  host: HTMLElement;
  rowHeight: number;
  ariaLabel: string;
  /** desenha uma linha; `selected` é a linha ativa */
  render: (item: T, index: number, selected: boolean) => HTMLElement;
  /** clique ou seta/Enter numa linha */
  onSelect: (item: T, index: number, via: "click" | "key") => void;
}

export interface VirtualList<T> {
  setItems(items: readonly T[], keepScroll?: boolean): void;
  /** marca a linha ativa (sem disparar `onSelect`) e a traz para a tela */
  setActive(index: number): void;
  refresh(): void;
  items(): readonly T[];
  dispose(): void;
}

export function createVirtualList<T>(opts: VirtualListOptions<T>): VirtualList<T> {
  const root = document.createElement("div");
  root.className = "vlist";
  root.tabIndex = 0;
  root.setAttribute("role", "listbox");
  root.setAttribute("aria-label", opts.ariaLabel);
  const spacer = document.createElement("div");
  spacer.className = "vlist-spacer";
  root.append(spacer);
  opts.host.replaceChildren(root);

  let data: readonly T[] = [];
  let active = -1;
  let frame = 0;
  const rows = new Map<number, HTMLElement>();

  function paint() {
    frame = 0;
    const [from, to] = visibleRange(root.scrollTop, root.clientHeight, opts.rowHeight, data.length);
    for (const [i, row] of rows) {
      if (i < from || i >= to) {
        row.remove();
        rows.delete(i);
      }
    }
    for (let i = from; i < to; i++) {
      if (rows.has(i)) continue;
      const row = opts.render(data[i]!, i, i === active);
      row.classList.add("vrow");
      row.style.position = "absolute";
      row.style.top = `${i * opts.rowHeight}px`;
      row.style.height = `${opts.rowHeight}px`;
      row.setAttribute("role", "option");
      row.setAttribute("aria-selected", String(i === active));
      row.dataset.index = String(i);
      spacer.append(row);
      rows.set(i, row);
    }
  }

  function schedule() {
    if (!frame) frame = requestAnimationFrame(paint);
  }

  function redrawAll() {
    for (const row of rows.values()) row.remove();
    rows.clear();
    paint();
  }

  function reveal(index: number) {
    const top = index * opts.rowHeight;
    if (top < root.scrollTop) root.scrollTop = top;
    else if (top + opts.rowHeight > root.scrollTop + root.clientHeight) root.scrollTop = top + opts.rowHeight - root.clientHeight;
  }

  function move(index: number, via: "click" | "key") {
    if (index < 0 || index >= data.length) return;
    active = index;
    reveal(index);
    // primeiro o chamador (que pode guardar a seleção ou refazer a lista), depois o destaque
    opts.onSelect(data[index]!, index, via);
    if (data[index]) redrawAll();
  }

  const onClick = (e: MouseEvent) => {
    const row = (e.target as HTMLElement).closest<HTMLElement>(".vrow");
    if (row) move(Number(row.dataset.index), "click");
  };
  const onKey = (e: KeyboardEvent) => {
    const page = Math.max(1, Math.floor(root.clientHeight / opts.rowHeight) - 1);
    const step: Record<string, number> = { ArrowDown: 1, ArrowUp: -1, PageDown: page, PageUp: -page };
    if (e.key in step) {
      e.preventDefault();
      move(Math.max(0, Math.min(data.length - 1, (active < 0 ? 0 : active) + (active < 0 ? 0 : step[e.key]!))), "key");
    } else if (e.key === "Home" || e.key === "End") {
      e.preventDefault();
      move(e.key === "Home" ? 0 : data.length - 1, "key");
    } else if (e.key === "Enter" && active >= 0) {
      e.preventDefault();
      opts.onSelect(data[active]!, active, "key");
    }
  };
  root.addEventListener("scroll", schedule, { passive: true });
  root.addEventListener("click", onClick);
  root.addEventListener("keydown", onKey);
  const resize = new ResizeObserver(schedule);
  resize.observe(root);

  return {
    setItems(items, keepScroll = false) {
      data = items;
      spacer.style.height = `${items.length * opts.rowHeight}px`;
      if (!keepScroll) root.scrollTop = 0;
      if (active >= items.length) active = -1;
      redrawAll();
    },
    setActive(index) {
      active = index;
      if (index >= 0) reveal(index);
      redrawAll();
    },
    refresh: redrawAll,
    items: () => data,
    dispose() {
      if (frame) cancelAnimationFrame(frame);
      resize.disconnect();
      root.remove();
      rows.clear();
    },
  };
}
