//! Atualização pelas releases do GitHub (tauri-plugin-updater). A webview só recebe dois comandos; o plugin fica no Rust.
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_updater::{Update, UpdaterExt};

/// A atualização achada pela última verificação; só é instalada se o usuário pedir.
#[derive(Default)]
pub struct PendingUpdate(Mutex<Option<Update>>);

#[derive(Serialize, Debug, PartialEq)]
pub struct UpdateInfo {
    pub current: String,
    /// Versão publicada mais nova, mesmo quando não é maior que a instalada.
    pub latest: String,
    pub available: bool,
    pub notes: String,
    pub date: Option<String>,
}

pub fn summarize(
    current: &str,
    published: Option<String>,
    found: Option<(&str, Option<String>)>,
) -> UpdateInfo {
    let available = found.is_some();
    let (notes, date) = found.map_or((String::new(), None), |(notes, date)| {
        (notes.trim().to_string(), date)
    });
    UpdateInfo {
        current: current.to_string(),
        latest: published.unwrap_or_else(|| current.to_string()),
        available,
        notes,
        date,
    }
}

fn failed(e: tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error as E;
    match e {
        E::Minisign(_) | E::Base64(_) | E::SignatureUtf8(_) => {
            "o instalador baixado não confere com a assinatura do projeto e foi descartado".into()
        }
        E::ReleaseNotFound => "nenhuma versão publicada foi encontrada".into(),
        E::EmptyEndpoints => "atualizações não configuradas nesta versão".into(),
        E::Reqwest(_) | E::Network(_) => "sem conexão com o servidor de atualizações".into(),
        other => other.to_string(),
    }
}

#[tauri::command]
pub async fn update_check(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> Result<UpdateInfo, String> {
    // Sem isto o rustls do plugin não acha provedor de TLS; é idempotente.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let current = app.package_info().version.to_string();
    let published: std::sync::Arc<Mutex<Option<String>>> = std::sync::Arc::default();
    let seen = published.clone();
    let update = app
        .updater_builder()
        .version_comparator(move |running, remote| {
            if let Ok(mut slot) = seen.lock() {
                *slot = Some(remote.version.to_string());
            }
            remote.version > running
        })
        .build()
        .map_err(failed)?
        .check()
        .await
        .map_err(failed)?;
    let published = published.lock().ok().and_then(|mut slot| slot.take());
    let info = summarize(
        &current,
        published,
        update.as_ref().map(|u| {
            (
                u.body.as_deref().unwrap_or_default(),
                u.date.map(|d| d.date().to_string()),
            )
        }),
    );
    if let Ok(mut slot) = pending.0.lock() {
        *slot = update;
    }
    Ok(info)
}

/// Baixa, confere a assinatura com a chave pública embutida, instala e reinicia o app.
#[tauri::command]
pub async fn update_install(
    app: AppHandle,
    pending: State<'_, PendingUpdate>,
) -> Result<(), String> {
    let update = pending
        .0
        .lock()
        .map_err(|_| "estado de atualização indisponível".to_string())?
        .take()
        .ok_or_else(|| "nenhuma atualização pendente: verifique de novo".to_string())?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(failed)?;
    app.restart()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_newer_release_is_offered_with_its_notes_and_date() {
        let info = summarize(
            "0.7.0",
            Some("0.8.0".into()),
            Some((" Correções\n", Some("2026-09-18".into()))),
        );
        assert_eq!(
            info,
            UpdateInfo {
                current: "0.7.0".into(),
                latest: "0.8.0".into(),
                available: true,
                notes: "Correções".into(),
                date: Some("2026-09-18".into()),
            }
        );
    }

    #[test]
    fn up_to_date_still_shows_the_published_version() {
        let info = summarize("0.8.0", Some("0.8.0".into()), None);
        assert!(!info.available);
        assert_eq!(info.latest, "0.8.0");
        assert_eq!(info.notes, "");
    }

    #[test]
    fn a_local_build_ahead_of_the_release_is_not_offered_a_downgrade() {
        let info = summarize("0.9.0-dev", Some("0.8.0".into()), None);
        assert!(!info.available);
        assert_eq!(
            info.latest, "0.8.0",
            "a tela deve dizer a versão publicada, não ecoar a local"
        );
    }

    #[test]
    fn a_tampered_download_is_explained_in_portuguese() {
        let msg = failed(tauri_plugin_updater::Error::SignatureUtf8("x".into()));
        assert!(msg.contains("não confere com a assinatura"), "{msg}");
        assert!(failed(tauri_plugin_updater::Error::ReleaseNotFound)
            .contains("nenhuma versão publicada"));
    }

    #[test]
    fn a_build_without_update_endpoints_says_so() {
        assert!(failed(tauri_plugin_updater::Error::EmptyEndpoints).contains("não configuradas"));
    }
}
