// Textos do painel de controles do visualizador de `.mdl` (aba Recursos): [pt, en].
// Entra em `DICT` por `...DICT_MODEL` (dict.ts), então vale o mesmo teste de paridade pt/en.
export const DICT_MODEL: Record<string, [string, string]> = {
  "mdl.panelAria": ["controles do modelo", "model controls"],
  "mdl.reloadFailed": ["não foi possível recarregar o modelo", "could not reload the model"],

  "mdl.bodygroups": ["bodygroups", "bodygroups"],
  "mdl.bodypartN": ["parte {0}", "part {0}"],

  "mdl.teamColors": ["cores de time", "team colors"],
  "mdl.teamColorsHint": [
    "matiz 0..255 das faixas de cima e de baixo das texturas de remap",
    "0..255 hue of the top and bottom ranges of the remap textures",
  ],
  "mdl.teamTop": ["cima", "top"],
  "mdl.teamBottom": ["baixo", "bottom"],
  "mdl.default": ["padrão", "default"],

  "mdl.controllers": ["controladores", "controllers"],
  "mdl.controllersHint": [
    "bone controllers: olhar/mira e boca; o valor mostrado é o que o byte do slider produz",
    "bone controllers: look/aim and mouth; the value shown is what the slider byte produces",
  ],
  "mdl.controllerN": ["controlador {0}", "controller {0}"],
  "mdl.mouth": ["boca", "mouth"],
  "mdl.resetControllers": ["repor", "reset"],

  "mdl.blends": ["blends", "blends"],
  "mdl.blendsHint": ["a sequência mistura {0} animações (mira/marcha)", "this sequence blends {0} animations (aim/gait)"],
  "mdl.blend": ["blend", "blend"],
  "mdl.blendN": ["blend {0}", "blend {0}"],

  "mdl.viewsAria": ["vistas do modelo", "model views"],
  "mdl.view.hero": ["3/4", "3/4"],
  "mdl.view.hero.hint": ["vista 3/4 da frente, enquadrando o modelo", "three-quarter front view, framing the model"],
  "mdl.view.front": ["frente", "front"],
  "mdl.view.front.hint": ["de frente para o modelo", "facing the model head-on"],
  "mdl.view.side": ["lado", "side"],
  "mdl.view.side.hint": ["de lado", "side view"],
  "mdl.view.back": ["costas", "back"],
  "mdl.view.back.hint": ["por trás do modelo", "behind the model"],
  "mdl.view.top": ["topo", "top"],
  "mdl.view.top.hint": ["de cima", "top-down"],
  "mdl.view.fit": ["enquadrar", "fit"],
  "mdl.view.fit.hint": ["reenquadra a câmera na pose atual", "re-fits the camera on the current pose"],

  "mdl.overlays": ["sobreposições", "overlays"],  "mdl.hitboxes": ["hitboxes ({0})", "hitboxes ({0})"],
  "mdl.attachments": ["attachments ({0})", "attachments ({0})"],
  "mdl.hitgroup0": ["grupo 0: genérico", "group 0: generic"],
  "mdl.hitgroup1": ["grupo 1: cabeça", "group 1: head"],
  "mdl.hitgroup2": ["grupo 2: peito", "group 2: chest"],
  "mdl.hitgroup3": ["grupo 3: estômago", "group 3: stomach"],
  "mdl.hitgroup4": ["grupo 4: braço esquerdo", "group 4: left arm"],
  "mdl.hitgroup5": ["grupo 5: braço direito", "group 5: right arm"],
  "mdl.hitgroup6": ["grupo 6: perna esquerda", "group 6: left leg"],
  "mdl.hitgroup7": ["grupo 7: perna direita", "group 7: right leg"],
  "mdl.hitgroupN": ["grupo {0}", "group {0}"],

  "mdl.events": ["eventos ({0})", "events ({0})"],
  "mdl.eventFrame": ["quadro {0}", "frame {0}"],
  "mdl.eventId": ["evento {0}", "event {0}"],
  "mdl.eventClient": ["tratado pelo cliente", "handled by the client"],
  "mdl.eventServer": ["tratado pelo servidor", "handled by the server"],

  "mdl.externalTextures": ["texturas lidas de nomeT.mdl", "textures read from nameT.mdl"],
  "mdl.warnings": ["avisos do modelo ({0})", "model warnings ({0})"],
};
