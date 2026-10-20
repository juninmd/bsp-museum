/**
 * Textos do painel "Camadas" e dos balões da vista 3D: `[português, inglês]`.
 * Entra no dicionário geral por `...DICT_VIEWER` (ver `dict.ts`).
 */
export const DICT_VIEWER: Record<string, [string, string]> = {
  "vw.layers": ["Camadas", "Layers"],
  "vw.note": [
    "Clique nos marcadores: sons mostram o raio; luzes ligáveis alternam. Passe o mouse em zonas e luzes para ver detalhes.",
    "Click markers: sounds show their radius; switchable lights toggle. Hover zones and lights for details.",
  ],

  // seções
  "vw.sec.surface": ["Superfície", "Surface"],
  "vw.sec.collision": ["Colisão (hulls)", "Collision (hulls)"],
  "vw.sec.volumes": ["Volumes e zonas", "Volumes and zones"],
  "vw.sec.scene": ["Cena", "Scene"],

  // camadas
  "vw.l.renderModes": ["Modos de render (aditivo, translúcido…)", "Render modes (additive, translucent…)"],
  "vw.l.scroll": ["Texturas que rolam (scroll)", "Scrolling textures (scroll)"],
  "vw.l.texAnim": ["Texturas animadas (+0…+9)", "Animated textures (+0…+9)"],
  "vw.l.texAlt": ["Quadros alternativos (+A…+J)", "Alternate frames (+A…+J)"],
  "vw.l.texAltHint": ["Sequência que o botão ligado usaria", "Sequence a pressed button would use"],
  "vw.l.lightAnim": ["Luz animada (lightstyles)", "Animated light (lightstyles)"],
  "vw.l.movers": ["Animar portas, plataformas e trens", "Animate doors, platforms and trains"],
  "vw.l.fog": ["Névoa (env_fog)", "Fog (env_fog)"],
  "vw.l.sun": ["Sol (light_environment)", "Sun (light_environment)"],
  "vw.l.sky": ["Céu", "Sky"],
  "vw.l.lights": ["Luzes", "Lights"],
  "vw.l.lightsHint": ["Esferas coloridas; luz ligável alterna ao clicar", "Colored spheres; switchable lights toggle on click"],
  "vw.l.sprites": ["Sprites e glows", "Sprites and glows"],
  "vw.l.sounds": ["Sons ambientes", "Ambient sounds"],
  "vw.l.soundsHint": ["Clique para ver arquivo e raio", "Click to see the file and radius"],
  "vw.l.decals": ["Decalques", "Decals"],
  "vw.l.routes": ["Rotas de trem", "Train routes"],
  "vw.l.volumes": ["Mostrar volumes", "Show volumes"],

  // hulls
  "vw.hull.btn": ["Hull {0}", "Hull {0}"],
  "vw.hull.n1": ["em pé", "standing"],
  "vw.hull.n2": ["grande", "large"],
  "vw.hull.n3": ["agachado", "crouching"],
  "vw.hull.size": ["{0} · corpo {1}", "{0} · body {1}"],
  "vw.hull.unavailable": ["Este mapa não tem esse hull", "This map has no such hull"],
  "vw.hull.progress": [
    "Hull {0}: {1} / {2} triângulos · janela {3}/{4}",
    "Hull {0}: {1} / {2} triangles · window {3}/{4}",
  ],
  "vw.hull.done": ["Hull {0}: {1} triângulos", "Hull {0}: {1} triangles"],
  "vw.hull.capped": [
    "Hull {0}: parou em {1} triângulos (limite de segurança)",
    "Hull {0}: stopped at {1} triangles (safety limit)",
  ],
  "vw.hull.truncated": ["pode estar incompleto (orçamento do backend)", "may be incomplete (backend work budget)"],
  "vw.hull.error": ["Hull {0}: falhou — {1}", "Hull {0}: failed — {1}"],

  // volumes
  "vw.vol.empty": ["Nenhum volume neste mapa.", "No volumes in this map."],
  "vw.vol.truncated": ["Parte da malha foi cortada por limite de triângulos.", "Part of the mesh was cut by a triangle limit."],
  "vw.vol.legendHint": ["Clique para ligar/desligar esta categoria", "Click to toggle this category"],
  "vw.vol.error": ["Falhou ao ler os volumes: {0}", "Could not read the volumes: {0}"],

  // sprites
  "vw.sprite.loading": ["Carregando sprites… {0}/{1}", "Loading sprites… {0}/{1}"],
  "vw.sprite.missing": ["{0} sprite(s) não encontrado(s): ficam como ícone.", "{0} sprite(s) not found: shown as an icon."],

  // zonas
  "vw.zone.trigger": ["gatilho", "trigger"],
  "vw.zone.buy_zone": ["zona de compra", "buy zone"],
  "vw.zone.bomb_target": ["alvo da bomba", "bomb target"],
  "vw.zone.hostage_rescue": ["resgate de reféns", "hostage rescue"],
  "vw.zone.vip_safety": ["zona segura do VIP", "VIP safety zone"],
  "vw.zone.escape_zone": ["zona de fuga", "escape zone"],
  "vw.zone.ladder": ["escada", "ladder"],
  "vw.zone.water": ["água", "water"],

  // balões
  "vw.tip.name": ["nome: {0}", "name: {0}"],
  "vw.tip.target": ["alvo: {0}", "target: {0}"],
  "vw.tip.message": ["mensagem: {0}", "message: {0}"],
  "vw.tip.invisible": ["superfície invisível · {0} face(s)", "invisible surface · {0} face(s)"],
  "vw.tip.light": ["luz", "light"],
  "vw.tip.spot": ["luz (spot)", "light (spot)"],
  "vw.tip.lightColor": ["cor RGB {0} · brilho {1}", "RGB {0} · brightness {1}"],
  "vw.tip.lightStyle": ["estilo {0} · padrão {1}", "style {0} · pattern {1}"],
  "vw.tip.lightSwitch": ["ligável — clique para ligar/desligar", "switchable — click to toggle"],
  "vw.tip.sound": ["som ambiente", "ambient sound"],
  "vw.tip.soundFile": ["arquivo: {0}", "file: {0}"],
  "vw.tip.radius": ["raio: {0} unidades", "radius: {0} units"],
  "vw.tip.radiusAll": ["raio: o mapa inteiro", "radius: whole map"],
  "vw.tip.volume": ["volume: {0}%", "volume: {0}%"],
  "vw.tip.silent": ["começa em silêncio", "starts silent"],
  "vw.tip.decal": ["textura: {0}", "texture: {0}"],
  "vw.tip.decalBrush": ["decalque em brush", "brush decal"],
  "vw.tip.decalPoint": ["decalque pontual (infodecal)", "point decal (infodecal)"],
  "vw.tip.spriteFile": ["sprite: {0}", "sprite: {0}"],
  "vw.tip.spriteRender": ["render: {0} ({1})", "render: {0} ({1})"],
};
