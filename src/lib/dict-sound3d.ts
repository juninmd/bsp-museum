/**
 * Textos da camada "Sons 3D" (áudio espacial) e do painel "Sons do mapa": `[português, inglês]`.
 * Entra no dicionário geral por `...DICT_SOUND3D` (ver `dict.ts`).
 */
export const DICT_SOUND3D: Record<string, [string, string]> = {
  "vw.l.sound3d": ["Sons 3D (áudio espacial)", "3D sounds (spatial audio)"],
  "vw.l.sound3dHint": [
    "Toca os ambient_generic a partir da câmera, com alcance e volume do mapa",
    "Plays the map's ambient_generic from the camera, with the map's range and volume",
  ],

  // painel
  "snd.title": ["Sons do mapa", "Map sounds"],
  "snd.collapse": ["recolher a lista", "collapse the list"],
  "snd.expand": ["expandir a lista", "expand the list"],
  "snd.enable": ["Ativar áudio", "Enable audio"],
  "snd.enableHint": [
    "O navegador só libera o som depois de um clique.",
    "The browser only unlocks sound after a click.",
  ],
  "snd.starting": ["iniciando…", "starting…"],
  "snd.error": ["Não foi possível iniciar o áudio: {0}", "Could not start audio: {0}"],
  "snd.master": ["Volume mestre", "Master volume"],
  "snd.muteAll": ["Mudo", "Mute"],
  "snd.repeat": ["Repetir sons sem laço", "Repeat sounds without a loop"],
  "snd.repeatHint": [
    "No jogo, um ambient_generic sem laço no WAV toca uma vez; aqui ele repete para dar para ouvir.",
    "In-game, an ambient_generic without a loop in the WAV plays once; here it repeats so you can hear it.",
  ],
  "snd.summary": ["{0}/{1} tocando · {2}", "{0}/{1} playing · {2}"],
  "snd.summaryHint": [
    "sons tocando / limite de vozes simultâneas · áudio decodificado em cache",
    "sounds playing / simultaneous voice limit · decoded audio in cache",
  ],
  "snd.sort": ["ordem", "order"],
  "snd.sort.map": ["do mapa", "map order"],
  "snd.sort.dist": ["mais próximos", "nearest first"],
  "snd.empty": ["Este mapa não tem ambient_generic.", "This map has no ambient_generic."],
  "snd.note": [
    "A câmera é o ouvinte. Fora do raio o som some; só os mais próximos tocam ao mesmo tempo.",
    "The camera is the listener. Outside the radius a sound fades out; only the nearest ones play at once.",
  ],

  // linha de um som
  "snd.focus": ["Ir até o emissor", "Go to the emitter"],
  "snd.mute": ["M", "M"],
  "snd.muteTitle": ["Mudo neste som", "Mute this sound"],
  "snd.solo": ["S", "S"],
  "snd.soloTitle": ["Solo: só os sons em solo tocam", "Solo: only soloed sounds play"],
  "snd.listen": ["Ouvir", "Hear"],
  "snd.listenStop": ["Parar", "Stop"],
  "snd.listenTitle": [
    "Ouvir este som isolado, sem atenuação por distância",
    "Hear this sound alone, without distance attenuation",
  ],
  "snd.dist": ["{0} u", "{0} u"],
  "snd.vol": ["vol. {0}%", "vol. {0}%"],
  "snd.radius": ["alcance {0} u", "range {0} u"],
  "snd.everywhere": ["em todo o mapa", "everywhere"],
  "snd.loop": ["laço", "loop"],
  "snd.silent": ["inicia silencioso", "starts silent"],
  "snd.level": ["nível do sinal", "signal level"],

  // estados
  "snd.st.idle": ["fora do alcance", "out of range"],
  "snd.st.near": ["no alcance", "in range"],
  "snd.st.loading": ["carregando…", "loading…"],
  "snd.st.playing": ["tocando", "playing"],
  "snd.st.done": ["terminou", "finished"],
  "snd.st.muted": ["mudo", "muted"],

  // por que um som não toca
  "snd.un.noFile": ["indisponível: sem arquivo (message vazio)", "unavailable: no file (empty message)"],
  "snd.un.sentence": [
    "indisponível: sentença do sentences.txt, não é um arquivo",
    "unavailable: a sentences.txt entry, not a file",
  ],
  "snd.un.missing": ["indisponível: arquivo não encontrado", "unavailable: file not found"],
  "snd.un.codec": ["indisponível: codec sem suporte ({0})", "unavailable: unsupported codec ({0})"],
  "snd.un.decode": ["indisponível: falha ao decodificar", "unavailable: decoding failed"],
  "snd.un.tooBig": ["indisponível: arquivo grande demais", "unavailable: file too large"],
};
