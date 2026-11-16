export const DICT_UPDATE: Record<string, [string, string]> = {
  "tools.update": ["verificar atualizações", "check for updates"],
  "update.title": ["Atualização", "Update"],
  "update.checking": ["verificando atualizações…", "checking for updates…"],
  "update.upToDate": ["versão {0} já é a mais recente", "version {0} is already the latest"],
  "update.available": ["nova versão {0} disponível: clique em ↻ para instalar", "new version {0} available: click ↻ to install"],
  "update.confirm": ["Instalar a versão {0} agora? O app será reiniciado.{1}", "Install version {0} now? The app will restart.{1}"],
  "update.installing": ["instalando a versão {0}…", "installing version {0}…"],
  "update.failed": ["atualização falhou: {0}", "update failed: {0}"],
};

Object.assign(DICT_UPDATE, {
  "tools.settings": ["configurações", "settings"],
  "settings.title": ["Configurações", "Settings"],
  "settings.version": ["Versão instalada", "Installed version"],
  "settings.language": ["Idioma", "Language"],
  "settings.theme": ["Tema", "Theme"],
  "settings.theme.dark": ["Escuro", "Dark"],
  "settings.theme.light": ["Claro", "Light"],
  "settings.auto": ["Verificar atualizações ao abrir", "Check for updates on startup"],
  "settings.check": ["Verificar agora", "Check now"],
  "settings.install": ["Instalar {0} e reiniciar", "Install {0} and restart"],
  "settings.updates": ["Atualizações", "Updates"],
  "settings.general": ["Geral", "General"],
  "update.downloading": ["baixando a versão {0}… {1}", "downloading version {0}… {1}"],
  "update.verifying": ["conferindo assinatura e instalando…", "verifying signature and installing…"],
  "update.newBadge": ["v{0} disponível", "v{0} available"],
});
