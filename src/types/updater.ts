/** Resultado de `update_check` (src-tauri/src/update.rs). */
export interface UpdateInfo {
  current: string;
  /** Versão publicada mais nova, mesmo quando não é maior que a instalada. */
  latest: string;
  available: boolean;
  notes: string;
  date: string | null;
}
