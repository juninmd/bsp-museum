import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import type { UpdateInfo, UpdateProgress } from "../types/updater.ts";

/** Consulta as releases do GitHub; falha com texto legível (sem rede, sem release, sem configuração). */
export const checkForUpdate = () => invoke<UpdateInfo>("update_check");

/** Confere a assinatura, instala e reinicia o app: só volta se algo falhar antes. */
export const installUpdate = () => invoke<void>("update_install");

/** Versão do app em execução (a da release instalada). */
export async function appVersion(): Promise<string> {
  try {
    return await getVersion();
  } catch {
    return "dev";
  }
}

/** Acompanha o download da atualização; devolve a função que cancela a escuta. */
export const onUpdateProgress = (cb: (p: UpdateProgress) => void) =>
  listen<UpdateProgress>("update-progress", (e) => cb(e.payload));

/** 0–100, ou null quando o servidor não informa o tamanho total. */
export function percent(p: Pick<UpdateProgress, "downloaded" | "total">): number | null {
  if (!p.total) return null;
  return Math.min(100, Math.round((p.downloaded / p.total) * 100));
}
