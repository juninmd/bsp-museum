/** Resultado de `update_check` (src-tauri/src/update.rs). */
export interface UpdateInfo {
  current: string;
  /** Versão publicada mais nova, mesmo quando não é maior que a instalada. */
  latest: string;
  available: boolean;
  notes: string;
  date: string | null;
}

/** Evento `update-progress` emitido durante o download (src-tauri/src/update.rs). */
export interface UpdateProgress {
  downloaded: number;
  total: number | null;
  done: boolean;
}
