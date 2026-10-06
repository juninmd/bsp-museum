/**
 * Textos em inglês dos achados do diagnóstico. O backend entrega o texto em
 * português + `args` (valores dinâmicos); em inglês o frontend remonta daqui.
 * `{0}`, `{1}`… são os `args` na ordem em que o backend os devolve.
 */
export interface FindingText {
  title: string;
  detail: string;
  hint: string;
}

export const FINDINGS_EN: Record<string, FindingText> = {
  "de-sem-bomb-target": {
    title: "de_ prefix without func_bomb_target",
    detail: "CS enters bomb mode from the file prefix, but there is nothing to plant on.",
    hint: "The round never ends by objective — only by time or elimination. Add func_bomb_target (or info_bomb_target), and an info_map_parameters if you want to tune the timing.",
  },
  "cs-sem-refem": {
    title: "cs_ prefix without hostage_entity",
    detail: "Hostage-rescue map with no hostage at all.",
    hint: "Without hostage_entity there is no objective. Add the hostages and the matching func_hostage_rescue.",
  },
  "cs-sem-resgate:warn": {
    title: "Hostages with no rescue zone and no CT spawn",
    detail: "{0} hostage(s), no rescue zone and no CT spawn.",
    hint: "GoldSrc rescues a hostage near any info_player_start — with no CT spawn the rescue has nowhere to converge.",
  },
  "cs-sem-resgate:info": {
    title: "Hostages without an explicit rescue zone",
    detail: "{0} hostage(s), no func_/info_hostage_rescue.",
    hint: "Not an error: with no zone, the engine rescues a hostage within 256u of any CT spawn (official GoldSrc fallback).",
  },
  "as-incompleto": {
    title: "as_ prefix is incomplete",
    detail: "info_vip_start={0}, func_vip_safetyzone={1}",
    hint: "VIP mode needs both: a VIP spawn and at least one safe zone.",
  },
  "prefixo-divergente": {
    title: "{0} entities in a {1} file",
    detail: "CS picks the mode from the file prefix, not from the content.",
    hint: "Rename the BSP to the right prefix, or the objective built into the map will never be used.",
  },
  "sem-spawn": {
    title: "No spawn points",
    detail: "Neither info_player_start nor info_player_deathmatch.",
    hint: "Nobody can join. This BSP does not run on a server.",
  },
  "spawn-de-um-time-so": {
    title: "Spawns for one team only",
    detail: "CT={0} · T={1}",
    hint: "The team without spawns cannot enter the game. In Zombie Plague maps this is sometimes intentional, but double-check.",
  },
  "poucos-spawns": {
    title: "{0} spawns for {1} slots",
    detail: "CT={2} · T={3}",
    hint: "With more players than spawns the server stacks people on the same point (telefrags and an FPS drop at round start).",
  },
  fullbright: {
    title: "Map has no lightmap (fullbright)",
    detail: "The lighting lump is empty.",
    hint: "Either RAD was not run at compile time, or the map leaked. Visually it is that flat, shadowless look.",
  },
  "sem-buyzone": {
    title: "Competitive map without func_buyzone",
    detail: "No buy zone found.",
    hint: "Without a buyzone players only get the starting pistol. In zombie maps this is usually intentional.",
  },
  "mapa-pesado": {
    title: "{0} MB",
    detail: "Large map for players downloading from a public server.",
    hint: "Above ~8 MB many people give up mid-download. Check what dominates the lumps.",
  },
  "wad-nao-declarado": {
    title: "{0} WAD texture(s), but the worldspawn lists none",
    detail: 'The worldspawn "wad" key is empty.',
    hint: "Players without the right WAD see everything pink and black. Either embed the textures or declare the WAD.",
  },
  "spawn-em-solido": {
    title: "{0} spawn(s) inside a wall or outside the map",
    detail: "First one at ({1}) — {2}.",
    hint: "The player spawns stuck. Move the info_player_* into playable space (and check that the map has not leaked).",
  },
  "sem-vis": {
    title: "No visibility (VIS) data",
    detail: "The visibility lump is empty in a map that has leaves.",
    hint: "Either the map leaked and VIS did not run, or it was compiled without VIS. Without it the engine draws everything all the time: FPS drops and the map gets heavier.",
  },
  "limite-motor": {
    title: "Engine limits ({0} over)",
    detail: "{1}",
    hint: "Going over the cap makes the map fail to load or the server crash; near the cap, any edit can break it. Simplify brushes, merge entities or split the map.",
  },
  "wad-nao-encontrado": {
    title: "No WAD found next to the map",
    detail: "Declared: {0}",
    hint: "WADs are looked up in the mod folder (the one containing maps/). Outside it the textures cannot be checked.",
  },
  "textura-ausente": {
    title: "{0} texture(s) that no WAD found has",
    detail: "{1}",
    hint: "Players see pink and black on those faces. Ship the WAD that contains them, or embed the textures in the BSP.",
  },
  "recurso-ausente": {
    title: "{0} referenced model(s)/sound(s)/sprite(s) missing",
    detail: "{1}",
    hint: "The map asks for these files and they are not in the mod folder. If they are base-game assets ignore this; if custom, they are missing from the server pack.",
  },
};
