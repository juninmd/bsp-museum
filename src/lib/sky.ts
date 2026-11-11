/** Skybox do GoldSrc: 6 imagens `gfx/env/<céu>{up,dn,lf,rt,ft,bk}.tga`. Parte pura (sem DOM). */

export const SKY_FACES = ["up", "dn", "lf", "rt", "ft", "bk"] as const;
export type SkyFace = (typeof SKY_FACES)[number];

export interface SkySet {
  /** pasta com o separador final (`/jogos/cstrike/gfx/env/`) */
  dir: string;
  /** nome do céu (`desert`) */
  prefix: string;
  /** extensão como veio no arquivo (`tga`, `TGA`, `bmp`) */
  ext: string;
  /** o lado do arquivo aberto */
  face: SkyFace;
  /** o sufixo estava em maiúsculas (`DESERTUP.TGA`): os irmãos seguem a mesma caixa */
  upper: boolean;
}

const NAME = /^(.+?)(up|dn|lf|rt|ft|bk)\.(tga|bmp)$/i;

/** O arquivo é de uma pasta `gfx/env` (relativo ao mod ou absoluto)? */
export function isSkyEnvPath(path: string): boolean {
  return /(^|[\\/])gfx[\\/]env[\\/][^\\/]+$/i.test(path);
}

/** Decompõe o caminho de um lado de céu; `null` se o nome não termina em up/dn/lf/rt/ft/bk. */
export function parseSkyPath(path: string): SkySet | null {
  if (!isSkyEnvPath(path)) return null;
  const cut = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  const dir = path.slice(0, cut + 1);
  const m = NAME.exec(path.slice(cut + 1));
  if (!m) return null;
  const suffix = m[2]!;
  return {
    dir,
    prefix: m[1]!,
    ext: m[3]!,
    face: suffix.toLowerCase() as SkyFace,
    upper: suffix === suffix.toUpperCase() && suffix !== suffix.toLowerCase(),
  };
}

/** Caminho do lado `face` com a extensão `ext` (por padrão a do arquivo aberto). */
export function skyFacePath(set: SkySet, face: SkyFace, ext: string = set.ext): string {
  return `${set.dir}${set.prefix}${set.upper ? face.toUpperCase() : face}.${ext}`;
}

/** Extensões a tentar, na ordem: a do arquivo aberto e depois a outra (o jogo aceita as duas). */
export function skyExtCandidates(ext: string): string[] {
  const lower = ext.toLowerCase();
  const other = lower === "tga" ? "bmp" : "tga";
  return [ext, lower === ext ? other : other.toUpperCase()];
}

/** Quantos dos 6 lados existem. */
export function foundCount(found: Record<SkyFace, boolean>): number {
  return SKY_FACES.filter((f) => found[f]).length;
}
