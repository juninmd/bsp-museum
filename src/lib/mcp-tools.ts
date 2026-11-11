/** Catálogo estático das ferramentas do servidor MCP `bsp-museum-mcp` (para a tela Claude Link). */

export type McpGroupId = "maps" | "files" | "visual";

export interface McpTool {
  name: string;
  /** devolve imagem (PNG) ao agente */
  image?: boolean;
  /** [pt, en] */
  desc: [string, string];
}

export interface McpGroup {
  id: McpGroupId;
  /** chave i18n do título */
  title: string;
  tools: McpTool[];
}

export const MCP_GROUPS: McpGroup[] = [
  {
    id: "maps",
    title: "claude.group.maps",
    tools: [
      { name: "list_maps", desc: ["Lista os .bsp de uma pasta com resumo barato.", "Lists the .bsp files in a folder with a cheap summary."] },
      { name: "map_summary", desc: ["Detalhe completo do mapa: modo, spawns, céu, WADs, lumps, achados.", "Full map detail: mode, spawns, sky, WADs, lumps, findings."] },
      { name: "map_diagnostics", desc: ["Todas as regras do diagnóstico, por gravidade.", "Every diagnostic rule, by severity."] },
      { name: "map_entities", desc: ["Entidades com chaves e posição; filtra por classe ou texto.", "Entities with keys and position; filter by class or text."] },
      { name: "map_resources", desc: ["O que o mapa precisa para rodar: FastDL, ausentes, tamanhos.", "What the map needs to run: FastDL, missing files, sizes."] },
      { name: "map_textures", desc: ["Texturas da tabela do BSP e de onde vêm (embutida, WAD, ausente).", "BSP texture table and where each comes from (embedded, WAD, missing)."] },
      { name: "map_scene", desc: ["Fog, céu, luzes, decals, sprites, sons, movers e rotas de trem.", "Fog, sky, lights, decals, sprites, sounds, movers and train paths."] },
      { name: "map_volumes", desc: ["Zonas e gatilhos com caixa; superfícies invisíveis por contagem.", "Zones and triggers with boxes; invisible surfaces by count."] },
      { name: "map_hulls_info", desc: ["Contagens de colisão: hulls 1 a 3, planos, nós, folhas.", "Collision counts: hulls 1-3, planes, nodes, leaves."] },
      { name: "map_nearby", desc: ["Entidades e superfícies perto de um ponto do mapa.", "Entities and surfaces near a point in the map."] },
      { name: "compare_maps", desc: ["Compara dois mapas: lumps, entidades, texturas, WADs e achados.", "Compares two maps: lumps, entities, textures, WADs and findings."] },
      { name: "audit_folder", desc: ["Auditoria da pasta inteira, com duplicados.", "Whole-folder audit, with duplicates."] },
      { name: "server_checklist", desc: ["Auditoria, ausentes, FastDL total e próximos passos.", "Audit, missing files, total FastDL and next steps."] },
    ],
  },
  {
    id: "files",
    title: "claude.group.files",
    tools: [
      { name: "list_roots", desc: ["Pastas que o servidor pode ler. Comece por aqui.", "Folders the server may read. Start here."] },
      { name: "mod_files", desc: ["Arquivos do mod, inclusive dentro de .pak.", "Mod files, including those inside .pak."] },
      { name: "mod_file_analyze", desc: ["Análise por formato: áudio, nav, demo, pak, liblist, texto.", "Per-format analysis: audio, nav, demo, pak, liblist, text."] },
      { name: "wad_list", desc: ["Diretório de um WAD2/WAD3: nome, tipo, dimensões.", "Directory of a WAD2/WAD3: name, type, size."] },
      { name: "wad_usage", desc: ["Quais mapas usam cada textura de um WAD.", "Which maps use each texture of a WAD."] },
      { name: "texture_usage", desc: ["Em quais mapas e WADs uma textura aparece.", "Which maps and WADs a texture appears in."] },
      { name: "model_info", desc: ["Bones, sequências, bodyparts, hitboxes e texturas de um .mdl.", "Bones, sequences, bodyparts, hitboxes and textures of a .mdl."] },
      { name: "sprite_info", image: true, desc: ["Cabeçalho e quadros de um .spr; com `frame`, a imagem.", "Header and frames of a .spr; with `frame`, the image."] },
      { name: "audio_info", desc: ["Metadados de WAV/MP3: codec, taxa, duração, loops.", "WAV/MP3 metadata: codec, rate, duration, loops."] },
      { name: "nav_info", desc: ["Áreas, conexões e lugares de um .nav dos bots.", "Areas, connections and places of a bot .nav."] },
      { name: "demo_info", desc: ["Cabeçalho e diretório de uma demo .dem.", "Header and directory of a .dem demo."] },
    ],
  },
  {
    id: "visual",
    title: "claude.group.visual",
    tools: [
      { name: "map_view", image: true, desc: ["Render 3D do mapa com texturas e lightmap: visão geral, planta, spawn ou câmera livre.", "3D render of the map with textures and lightmap: overview, plan, spawn or free camera."] },
      { name: "map_radar", image: true, desc: ["Radar verde do mapa; grava overviews/ só com --allow-write.", "Green radar of the map; writes overviews/ only with --allow-write."] },
      { name: "map_texture_sheet", image: true, desc: ["Folha com as texturas que o mapa usa.", "Contact sheet of the textures the map uses."] },
      { name: "map_skybox", image: true, desc: ["Os 6 lados do céu do mapa numa imagem.", "The map's 6 sky sides in one image."] },
      { name: "map_lightmap", image: true, desc: ["Atlas de lightmaps do mapa.", "The map's lightmap atlas."] },
      { name: "model_view", image: true, desc: ["Modelo .mdl com skin, em repouso ou num quadro de sequência.", "A .mdl model with skin, at rest or on a sequence frame."] },
      { name: "model_turnaround", image: true, desc: ["O modelo visto de vários ângulos numa imagem.", "The model seen from several angles in one image."] },
      { name: "model_filmstrip", image: true, desc: ["Quadros de uma sequência lado a lado.", "Frames of a sequence side by side."] },
      { name: "model_texture", image: true, desc: ["Textura (skin) de um modelo.", "A model's texture (skin)."] },
      { name: "sprite_sheet", image: true, desc: ["Todos os quadros de um .spr numa folha.", "All frames of a .spr on one sheet."] },
      { name: "wad_texture", image: true, desc: ["Uma textura de WAD como imagem.", "A WAD texture as an image."] },
      { name: "wad_sheet", image: true, desc: ["Folha de contato das texturas de um WAD.", "Contact sheet of a WAD's textures."] },
      { name: "image_view", image: true, desc: ["TGA, BMP ou LMP decodificado (céu, spray, HUD).", "Decoded TGA, BMP or LMP (sky, spray, HUD)."] },
      { name: "audio_waveform", image: true, desc: ["Forma de onda de um WAV como imagem.", "A WAV's waveform as an image."] },
    ],
  },
];

/** Todos os nomes, na ordem dos grupos. */
export function allToolNames(): string[] {
  return MCP_GROUPS.flatMap((g) => g.tools.map((t) => t.name));
}

/** Filtra por trecho do nome ou da descrição (sem acento, sem caixa); grupos vazios saem. */
export function filterGroups(query: string, lang: 0 | 1): McpGroup[] {
  const norm = (s: string) => s.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();
  const q = norm(query.trim());
  if (!q) return MCP_GROUPS;
  return MCP_GROUPS.map((g) => ({
    ...g,
    tools: g.tools.filter((t) => norm(t.name).includes(q) || norm(t.desc[lang]).includes(q)),
  })).filter((g) => g.tools.length);
}

/** Comando que registra o servidor no Claude Code. `root` vazio usa o marcador. */
export function addCommand(root: string | null): string {
  const r = root && root.trim() ? root.trim() : "/caminho/cstrike";
  const q = /\s/.test(r) ? `"${r}"` : r;
  return `claude mcp add bsp-museum -- /caminho/bsp-museum-mcp serve --root ${q}`;
}

/** Bloco `.mcp.json` equivalente ao comando. */
export function mcpJson(root: string | null): string {
  const r = root && root.trim() ? root.trim() : "/caminho/cstrike";
  return JSON.stringify({ mcpServers: { "bsp-museum": { command: "/caminho/bsp-museum-mcp", args: ["serve", "--root", r] } } }, null, 2);
}
