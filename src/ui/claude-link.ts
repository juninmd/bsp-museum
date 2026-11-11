// Seção "Claude Link": como conectar o Claude Code ao servidor MCP e o que ele pode fazer.
// Estática de propósito: o servidor roda no cliente MCP, então o app não tem como mostrar chamadas ao vivo.

import { getLang, t } from "../i18n.ts";
import { modRootFromMapsDir } from "../lib/explorer.ts";
import { addCommand, filterGroups, mcpJson, MCP_GROUPS } from "../lib/mcp-tools.ts";
import { copyWithFeedback, esc } from "./explorer/common.ts";
import { state } from "./explorer/state.ts";

export interface ClaudeHooks {
  /** `settings.last_dir` (costuma ser `<mod>/maps`) */
  getLastDir(): string | null;
}

export interface ClaudeController {
  show(): void;
  hide(): void;
  refreshLanguage(): void;
}

const TOTAL = MCP_GROUPS.reduce((n, g) => n + g.tools.length, 0);

export function initClaudeLink(section: HTMLElement, hooks: ClaudeHooks): ClaudeController {
  let visible = false;
  let query = "";

  const modRoot = (): string | null => {
    if (state.modRoot) return state.modRoot;
    const last = hooks.getLastDir();
    return last ? modRootFromMapsDir(last) : null;
  };

  function toolsHtml(): string {
    const groups = filterGroups(query, getLang() === "pt" ? 0 : 1);
    if (!groups.length) return `<p class="claude-none dim">${esc(t("claude.none"))}</p>`;
    const lang = getLang() === "pt" ? 0 : 1;
    return groups
      .map(
        (g) => `<section class="claude-group" data-group="${g.id}">
          <h3>${esc(t(g.title))} <span class="dim">${g.tools.length}</span></h3>
          <ul>${g.tools
            .map(
              (tool) => `<li class="claude-tool">
                <code>${esc(tool.name)}</code>${tool.image ? `<span class="claude-badge">${esc(t("claude.badge.image"))}</span>` : ""}
                <p>${esc(tool.desc[lang])}</p></li>`,
            )
            .join("")}</ul></section>`,
      )
      .join("");
  }

  function render() {
    if (!visible) return;
    const root = modRoot();
    section.innerHTML = `
      <header class="claude-hero">
        <div>
          <div class="eyebrow">${esc(t("claude.eyebrow"))}</div>
          <h2 class="hero-title">${esc(t("claude.title"))}</h2>
          <p class="hero-sub">${esc(t("claude.sub"))}</p>
        </div>
      </header>
      <p class="claude-honest">${esc(t("claude.honest"))}</p>
      <div class="claude-grid">
        <section class="claude-card" aria-labelledby="claude-connect">
          <h3 id="claude-connect">${esc(t("claude.connect"))}</h3>
          <p class="claude-step">${esc(t("claude.step1"))}</p>
          <div class="claude-code"><code>cd src-tauri &amp;&amp; cargo build --release --no-default-features --bin bsp-museum-mcp</code>
            <button type="button" class="ghost2 claude-copy" data-copy="cd src-tauri && cargo build --release --no-default-features --bin bsp-museum-mcp">${esc(t("claude.copy"))}</button></div>
          <p class="claude-step">${esc(t("claude.step2"))}</p>
          <div class="claude-code"><code>${esc(addCommand(root))}</code>
            <button type="button" class="ghost2 claude-copy" data-copy="${esc(addCommand(root))}">${esc(t("claude.copy"))}</button></div>
          <p class="claude-step">${esc(t("claude.step3"))}</p>
          <div class="claude-code is-block"><pre>${esc(mcpJson(root))}</pre>
            <button type="button" class="ghost2 claude-copy" data-copy="${esc(mcpJson(root))}">${esc(t("claude.copy"))}</button></div>
          <p class="dim small">${esc(root ? t("claude.rootNote") : t("claude.noRoot"))}</p>
        </section>
        <section class="claude-card" aria-labelledby="claude-prompt">
          <h3 id="claude-prompt">${esc(t("claude.prompt.title"))}</h3>
          <p>${esc(t("claude.prompt.desc"))}</p>
          <p class="claude-step">${esc(t("claude.prompt.use"))}</p>
          <div class="claude-code"><code>/mcp__bsp-museum__tour_map</code>
            <button type="button" class="ghost2 claude-copy" data-copy="/mcp__bsp-museum__tour_map">${esc(t("claude.copy"))}</button></div>
        </section>
      </div>
      <section class="claude-tools" aria-labelledby="claude-tools-h">
        <header class="claude-tools-head">
          <h3 id="claude-tools-h">${esc(t("claude.tools"))} <span class="dim">${esc(t("claude.tools.count", TOTAL))}</span></h3>
          <input type="search" class="claude-search" placeholder="${esc(t("claude.search"))}" aria-label="${esc(t("claude.search"))}" autocomplete="off" value="${esc(query)}" />
        </header>
        <div class="claude-list">${toolsHtml()}</div>
      </section>`;

    for (const btn of section.querySelectorAll<HTMLButtonElement>(".claude-copy")) {
      btn.addEventListener("click", () => void copyWithFeedback(btn, btn.dataset.copy ?? ""));
    }
    const input = section.querySelector<HTMLInputElement>(".claude-search")!;
    input.addEventListener("input", () => {
      query = input.value;
      section.querySelector<HTMLElement>(".claude-list")!.innerHTML = toolsHtml();
    });
  }

  return {
    show() {
      visible = true;
      render();
    },
    hide() {
      visible = false;
    },
    refreshLanguage() {
      render();
    },
  };
}
