import { appVersion, checkForUpdate, installUpdate, onUpdateProgress, percent } from "../lib/updates.ts";
import type { UpdateInfo } from "../types/updater.ts";
import { t } from "../i18n.ts";
import { escapeHtml } from "./dom.ts";
import { openModal } from "./modal.ts";

export interface SettingsHooks {
  toggleLang(): Promise<void>;
  toggleTheme(): Promise<void>;
  setAutoUpdate(on: boolean): Promise<void>;
  autoUpdate(): boolean;
  isDark(): boolean;
  langLabel(): string;
  /** Atualização já conhecida (da verificação automática), se houver. */
  known(): UpdateInfo | null;
  setStatus(text: string, tone?: "info" | "error"): void;
}

/** Tela de configurações: versão, idioma, tema e atualização com progresso visível. */
export async function openSettings(h: SettingsHooks): Promise<void> {
  const { body } = openModal(t("settings.title"));
  const version = await appVersion();
  body.innerHTML = `
    <section class="settings">
      <h3>${escapeHtml(t("settings.general"))}</h3>
      <div class="set-row"><span>${escapeHtml(t("settings.version"))}</span><strong id="set-version">v${escapeHtml(version)}</strong></div>
      <div class="set-row"><span>${escapeHtml(t("settings.language"))}</span><button id="set-lang" class="ghost">${escapeHtml(h.langLabel())}</button></div>
      <div class="set-row"><span>${escapeHtml(t("settings.theme"))}</span><button id="set-theme" class="ghost">${escapeHtml(h.isDark() ? t("settings.theme.dark") : t("settings.theme.light"))}</button></div>
      <h3>${escapeHtml(t("settings.updates"))}</h3>
      <label class="set-row"><span>${escapeHtml(t("settings.auto"))}</span><input id="set-auto" type="checkbox" ${h.autoUpdate() ? "checked" : ""}></label>
      <div class="set-row"><button id="set-check" class="ghost">${escapeHtml(t("settings.check"))}</button><span id="set-msg" class="dim" role="status" aria-live="polite"></span></div>
      <div id="set-notes" class="dim small" hidden></div>
      <progress id="set-bar" max="100" hidden></progress>
      <button id="set-install" hidden></button>
    </section>`;
  const $ = <T extends HTMLElement>(id: string) => body.querySelector<T>(`#${id}`)!;
  const msg = $("set-msg");
  const bar = $<HTMLProgressElement>("set-bar");
  const install = $<HTMLButtonElement>("set-install");
  const check = $<HTMLButtonElement>("set-check");

  const showUpdate = (info: UpdateInfo) => {
    if (!info.available) {
      msg.textContent = t("update.upToDate", info.current);
      return;
    }
    msg.textContent = t("update.newBadge", info.latest);
    $("set-notes").textContent = info.notes;
    $("set-notes").hidden = !info.notes;
    install.textContent = t("settings.install", info.latest);
    install.hidden = false;
    install.onclick = async () => {
      install.disabled = check.disabled = true;
      bar.hidden = false;
      bar.removeAttribute("value");
      const off = await onUpdateProgress((p) => {
        const pct = percent(p);
        if (p.done) {
          msg.textContent = t("update.verifying");
          return;
        }
        if (pct !== null) bar.value = pct;
        const txt = t("update.downloading", info.latest, pct === null ? "" : `${pct}%`);
        msg.textContent = txt;
        h.setStatus(txt);
      });
      try {
        await installUpdate();
      } catch (e) {
        msg.textContent = t("update.failed", String(e));
        h.setStatus(t("update.failed", String(e)), "error");
        bar.hidden = true;
        install.disabled = check.disabled = false;
      } finally {
        off();
      }
    };
  };

  const known = h.known();
  if (known) showUpdate(known);
  check.onclick = async () => {
    msg.textContent = t("update.checking");
    try {
      showUpdate(await checkForUpdate());
    } catch (e) {
      msg.textContent = t("update.failed", String(e));
    }
  };
  $("set-lang").onclick = async () => {
    await h.toggleLang();
    void openSettings(h);
  };
  $("set-theme").onclick = async () => {
    await h.toggleTheme();
    $("set-theme").textContent = h.isDark() ? t("settings.theme.dark") : t("settings.theme.light");
  };
  $<HTMLInputElement>("set-auto").onchange = (e) => void h.setAutoUpdate((e.target as HTMLInputElement).checked);
}
