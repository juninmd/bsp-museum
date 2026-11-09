import { invoke } from "@tauri-apps/api/core";
import type { UpdateInfo } from "../types/updater.ts";

/** Consulta as releases do GitHub; falha com texto legível (sem rede, sem release, sem configuração). */
export const checkForUpdate = () => invoke<UpdateInfo>("update_check");

/** Confere a assinatura, instala e reinicia o app: só volta se algo falhar antes. */
export const installUpdate = () => invoke<void>("update_install");
