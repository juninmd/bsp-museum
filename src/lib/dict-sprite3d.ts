// Textos da visualização 3D dos sprites `.spr` (Explorador): [pt, en].
// Entra em `DICT` por `...DICT_SPRITE3D` (dict.ts), então vale o teste de paridade pt/en.
export const DICT_SPRITE3D: Record<string, [string, string]> = {
  "exp.spr3d.mode": ["modo de exibição", "display mode"],
  "exp.spr3d.2d": ["2D", "2D"],
  "exp.spr3d.3d": ["3D", "3D"],
  "exp.spr3d.stageAria": [
    "cena 3D do sprite: arraste para girar, role para aproximar",
    "3D sprite scene: drag to orbit, scroll to zoom",
  ],
  "exp.spr3d.panelAria": ["controles da visualização 3D", "3D view controls"],
  "exp.spr3d.noWebgl": ["WebGL indisponível neste ambiente.", "WebGL is not available in this environment."],

  "exp.spr3d.view": ["visão", "view"],
  "exp.spr3d.view.game": ["câmera do jogo", "game camera"],
  "exp.spr3d.view.observer": ["observador", "observer"],
  "exp.spr3d.autoRotate": ["girar a câmera automaticamente", "rotate the camera automatically"],
  "exp.spr3d.camRoll": ["rolagem da câmera", "camera roll"],
  "exp.spr3d.showCamera": ["mostrar a câmera do mundo", "show the world camera"],
  "exp.spr3d.showSphere": ["esfera do raio", "radius sphere"],
  "exp.spr3d.resetCamera": ["repor câmera", "reset camera"],

  "exp.spr3d.scale": ["escala", "scale"],
  "exp.spr3d.renderamt": ["renderamt (opacidade)", "renderamt (opacity)"],
  "exp.spr3d.rendercolor": ["rendercolor (cor)", "rendercolor (color)"],

  "exp.spr3d.bg": ["fundo 3D", "3D background"],
  "exp.spr3d.bg.dark": ["escuro", "dark"],
  "exp.spr3d.bg.light": ["claro", "light"],
  "exp.spr3d.bg.sky": ["céu", "sky"],

  "exp.spr3d.angles": ["angles (entidade)", "angles (entity)"],
  "exp.spr3d.pitch": ["pitch", "pitch"],
  "exp.spr3d.yaw": ["yaw", "yaw"],
  "exp.spr3d.roll": ["roll", "roll"],
  "exp.spr3d.anglesRollOnly": ["só o roll vale neste tipo", "only roll applies to this type"],

  "exp.spr3d.legend": [
    "eixos: X vermelho (frente), Y verde (esquerda), Z azul (cima) · esfera = raio de colisão {0} · escala {1}",
    "axes: X red (forward), Y green (left), Z blue (up) · sphere = bounding radius {0} · scale {1}",
  ],
  "exp.spr3d.degenerate": [
    "O motor não desenha este sprite com a câmera quase vertical (eixos indefinidos).",
    "The engine does not draw this sprite when the camera is almost vertical (undefined axes).",
  ],

  "exp.spr3d.rule.parallel_upright": [
    "paralelo vertical: sobe sempre em Z e gira só em Z acompanhando a direção do olhar; some com o olhar quase vertical.",
    "parallel upright: always up along Z and turns only around Z following the view direction; vanishes when looking almost straight up or down.",
  ],
  "exp.spr3d.rule.facing_upright": [
    "de frente vertical: sobe em Z e gira só em Z para encarar a posição da câmera (não a direção do olhar).",
    "facing upright: up along Z and turns only around Z to face the camera position (not the view direction).",
  ],
  "exp.spr3d.rule.parallel": [
    "paralelo à tela: sempre de frente para a câmera, inclusive com a rolagem dela.",
    "parallel: always faces the camera, including the camera roll.",
  ],
  "exp.spr3d.rule.oriented": [
    "orientado: plano fixo no mundo, definido pelos angles da entidade; não acompanha a câmera.",
    "oriented: plane fixed in the world, set by the entity angles; does not follow the camera.",
  ],
  "exp.spr3d.rule.parallel_oriented": [
    "paralelo orientado: de frente para a câmera, girado pelo roll dos angles.",
    "parallel oriented: faces the camera, rotated by the roll of the angles.",
  ],
};
