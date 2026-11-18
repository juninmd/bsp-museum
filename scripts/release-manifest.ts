export type Platform = { signature: string; url: string };
export type Manifest = { version: string; notes: string; pub_date?: string; platforms: Record<string, Platform> };
export type Release = { tag_name: string; draft: boolean; body: string; assets: { id: number; name: string }[] };

const updaterBundles: Record<string, RegExp> = {
  "windows-x86_64": /_x64-setup\.exe$/,
  "windows-x86_64-nsis": /_x64-setup\.exe$/,
  "windows-x86_64-msi": /_x64(?:_en-US)?\.msi$/,
  "darwin-aarch64": /_aarch64\.app\.tar\.gz$/,
  "darwin-aarch64-app": /_aarch64\.app\.tar\.gz$/,
  "darwin-x86_64": /_x64\.app\.tar\.gz$/,
  "darwin-x86_64-app": /_x64\.app\.tar\.gz$/,
  "linux-x86_64": /_(?:amd64|x86_64)\.AppImage$/,
  "linux-x86_64-appimage": /_(?:amd64|x86_64)\.AppImage$/,
  "linux-x86_64-deb": /_(?:amd64|x86_64)\.deb$/,
  "linux-x86_64-rpm": /[._](?:x86_64|amd64)\.rpm$/,
};

function bundleName(assets: string[], version: string, pattern: RegExp, signed: boolean): string | undefined {
  return assets.find((name) => name.includes(version) && pattern.test(name) && (!signed || assets.includes(`${name}.sig`)));
}

/**
 * `latest.json` only exists when the build had the updater signing key. Without it the release is published with the
 * plain installers (the app has nothing to update from), so signatures are not required.
 */
export function hasRequiredAssets(assets: string[], version: string): boolean {
  const signed = assets.includes("latest.json");
  return Object.values(updaterBundles).every((pattern) => bundleName(assets, version, pattern, signed));
}

export function createReleaseManifest(
  release: Release,
  signatures: ReadonlyMap<string, string>,
  tag: string,
  version: string,
  notes: string,
  repository: string,
  publishedAt = new Date().toISOString(),
): Manifest {
  if (release.tag_name !== tag || !release.draft || !notes.trim()) throw new Error("Tag, rascunho ou changelog inválido");
  const names = release.assets.map(({ name }) => name);
  const prefix = `https://api.github.com/repos/${repository}/releases/assets/`;
  const platforms = Object.fromEntries(Object.entries(updaterBundles).map(([platform, pattern]) => {
    const name = bundleName(names, version, pattern, true);
    const asset = release.assets.find((item) => item.name === name);
    const signature = name ? signatures.get(`${name}.sig`)?.trim() : undefined;
    if (!asset || !signature) throw new Error(`Instalador ou assinatura ausente para ${platform}`);
    return [platform, { signature, url: `${prefix}${asset.id}` }];
  }));
  return { version, notes, pub_date: publishedAt, platforms };
}

export function verifyReleaseManifest(manifest: Manifest, release: Release, tag: string, version: string, repository: string): void {
  if (release.tag_name !== tag || !release.draft || manifest.version !== version || !manifest.notes?.trim()) {
    throw new Error("Tag, rascunho, versão ou changelog inválido");
  }
  if (manifest.notes.trim() !== release.body?.trim()) throw new Error("Changelog do updater difere da release");
  const names = release.assets.map((asset) => asset.name);
  if (!hasRequiredAssets(names, version)) throw new Error("Instaladores incompletos");
  const prefix = `https://api.github.com/repos/${repository}/releases/assets/`;
  for (const [platform, pattern] of Object.entries(updaterBundles)) {
    const entry = manifest.platforms?.[platform];
    const id = entry?.url?.startsWith(prefix) ? entry.url.slice(prefix.length) : "";
    const asset = /^\d+$/.test(id) ? release.assets.find((item) => item.id === Number(id)) : undefined;
    if (!entry?.signature || !asset || asset.name !== bundleName(names, version, pattern, true)) {
      throw new Error(`Updater inválido para ${platform}`);
    }
  }
}
