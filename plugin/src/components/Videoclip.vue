<script setup lang="ts">
import Opciones from "./Opciones.vue";
import ListaVideos from "./ListaVideos.vue";
import { useConfig, vistasAbiertas } from "../main";
import { aCancion, appleDe, consultaMv, musicKit, type Cancion, type VideoApple } from "../cancion";

type Video = { videoId: string; streamUrl: string; videoTitle?: string; channel?: string; offset: number; origen: string; descartados?: number; recorte: number };
type Resultado = { id: string; title: string; channel: string; duration: number | null };

const GRANO =
  "url(\"data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='160' height='160'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='2' stitchTiles='stitch'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E\")";

const cfg = useConfig();

const cancion = ref<Cancion | null>(null);
const video = ref<Video | null>(null);
const estado = ref<"inactivo" | "buscando" | "listo" | "apple" | "sin-video" | "error">("inactivo");
const apple = ref<VideoApple | null>(null);
const elApple = ref<HTMLDivElement | null>(null);
const mensaje = ref("");
const panel = ref(false);
const pestana = ref<"cancion" | "ajustes" | "videos">("cancion");
const paleta = ref<Record<string, string> | null>(null);
const consulta = ref("");
const resultados = ref<Resultado[]>([]);
const buscando = ref(false);
const inactivo = ref(false);
const estrecho = ref(false);
const elRaiz = ref<HTMLDivElement | null>(null);
let observador: ResizeObserver | null = null;
const reproduciendo = ref(false);
// Video ya resuelto de la canción que viene; su elemento queda oculto y cargando.
const siguiente = ref<{ clave: string; video: Video } | null>(null);
const videos = computed(() => {
  const lista = video.value ? [video.value] : [];
  if (siguiente.value && siguiente.value.video.streamUrl !== video.value?.streamUrl) lista.push(siguiente.value.video);
  return lista;
});
const elAmbiente = ref<HTMLCanvasElement | null>(null);

let reloj = 0;
let pulso = 0;
let peticion: AbortController | null = null;
let reintentado = false;
let ultimaActividad = Date.now();
let esperandoDesde = 0;
let pausadoPorCarga = false;
let precargada = "";
let inicioCambio = 0;
// Último videoclip de Apple que sonó; al volver de él a su canción no se salta otra vez al video.
let ultimoApple = "";
const elementos = new Map<string, HTMLVideoElement>();

function registrar(v: Video, el: unknown) {
  if (el instanceof HTMLVideoElement) elementos.set(v.streamUrl, el);
  else elementos.delete(v.streamUrl);
}

function elActual() {
  return video.value ? (elementos.get(video.value.streamUrl) ?? null) : null;
}

function tienda(): any {
  return (globalThis as any).__PLUGINSYS__?.Stores?.appleMusicStore ?? null;
}

function tiempoAudio(): number {
  const t = tienda()?.audioElement?.currentTime;
  if (typeof t === "number" && Number.isFinite(t)) return t;
  const mk = musicKit()?.currentPlaybackTime;
  return typeof mk === "number" && Number.isFinite(mk) ? mk : 0;
}

function sonando(): boolean {
  const s = tienda();
  if (typeof s?.isPlaying === "boolean") return s.isPlaying;
  return musicKit()?.isPlaying === true;
}

// Cider reproduce los videoclips de Apple en un elemento propio; se trae a nuestro marco mientras dure.
function adoptarApple() {
  const v = document.getElementById("apple-music-video-player");
  const destino = elApple.value;
  if (!v || !destino || v.parentElement === destino) return;
  destino.replaceChildren(v);
  document.querySelector<HTMLElement>(".videoPlayerContainer")?.style.setProperty("display", "none");
  if (sonando() === false && reproduciendo.value) musicKit()?.play?.();
}

function soltarApple() {
  const v = document.getElementById("apple-music-video-player");
  const origen = document.getElementById("apple-music-video-container");
  if (v && origen && v.parentElement !== origen) origen.appendChild(v);
  document.querySelector<HTMLElement>(".videoPlayerContainer")?.style.removeProperty("display");
}

async function buscarApple(c: Cancion) {
  apple.value = null;
  const hallado = await appleDe(c);
  if (cancion.value?.clave === c.clave) apple.value = hallado;
}

async function verEnApple() {
  const mk = musicKit();
  if (!mk || !apple.value) return false;
  const momento = tiempoAudio();
  const id = apple.value.id;
  try {
    try {
      // Si el videoclip ya está en la cola justo después, se salta a él en vez de insertarlo otra vez.
      const siguienteEnCola = mk.queue?.items?.[(mk.queue?.position ?? -2) + 1];
      if (siguienteEnCola?.type !== "musicVideo" || String(siguienteEnCola.id) !== id) await mk.playNext({ musicVideo: id });
      await mk.skipToNextItem();
    } catch {
      // Desde una emisora o lista automática no se puede insertar: se rehace la cola con el video delante.
      const cola = mk.queue;
      const resto = (cola?.items ?? [])
        .slice((cola?.position ?? 0) + 1)
        .filter((i: any) => i.type !== "musicVideo" && i.container?.type !== "stations")
        .map((i: any) => String(i.attributes?.playParams?.catalogId ?? i.id));
      await mk.setQueue({ musicVideo: id, startPlaying: true });
      if (resto.length) await mk.playLater({ songs: resto }).catch(() => {});
    }
    if (momento > 8) await mk.seekToTime(momento);
    panel.value = false;
    return true;
  } catch (e: any) {
    mensaje.value = `Apple no pudo reproducir el videoclip: ${e?.message ?? e}`;
    avisar(mensaje.value);
    return false;
  }
}

// Con la preferencia activa, la canción se cambia por su videoclip de Apple; si no tiene, se usa YouTube.
// Las dos consultas salen a la vez para no esperar a Apple antes de empezar con YouTube.
async function iniciar(c: Cancion) {
  const listo = siguiente.value?.clave === c.clave ? siguiente.value.video : null;
  if (listo) avisar(`${c.titulo}: video precargado`);
  siguiente.value = null;
  peticion?.abort();
  peticion = new AbortController();
  estado.value = "buscando";
  mensaje.value = "";
  video.value = null;
  const youtube = listo ? Promise.resolve(listo) : pedirMv(c, peticion.signal);
  youtube.catch(() => {});

  await buscarApple(c);
  if (cancion.value?.clave !== c.clave) return;
  const vuelve = apple.value?.id === ultimoApple;
  ultimoApple = "";
  if (cfg.preferirApple && apple.value && !vuelve && (await verEnApple())) {
    peticion.abort();
    return;
  }
  cargarVideo(c, youtube);
}

function avisar(texto: string) {
  fetch(`${cfg.servicio}/log`, { method: "POST", body: texto }).catch(() => {});
}

async function pedirMv(c: Cancion, senal?: AbortSignal): Promise<Video | null> {
  const r = await fetch(`${cfg.servicio}/mv?${consultaMv(c, cfg)}`, { signal: senal });
  const datos = await r.json();
  if (!r.ok) throw new Error(datos?.error ?? `HTTP ${r.status}`);
  return datos.status === "ok" ? { ...datos, offset: Number(datos.offset) || 0, recorte: Number(datos.recorte) || 0 } : null;
}

function mostrar(c: Cancion, resultado: Video | null) {
  if (cancion.value?.clave !== c.clave) return;
  video.value = resultado;
  estado.value = resultado ? "listo" : "sin-video";
  avisar(`${c.artista} - ${c.titulo}: ${resultado ? resultado.videoId : "sin video"} en ${Math.round(performance.now() - inicioCambio)} ms`);
  precargarSiguiente();
}

function fallo(e: any) {
  if (e?.name === "AbortError") return;
  estado.value = "error";
  mensaje.value = e instanceof TypeError ? "No se pudo conectar con el servicio maku." : String(e?.message ?? e);
}

async function cargarVideo(c: Cancion, pendiente?: Promise<Video | null>) {
  if (!pendiente) {
    peticion?.abort();
    peticion = new AbortController();
  }
  video.value = null;
  estado.value = "buscando";
  mensaje.value = "";
  try {
    mostrar(c, await (pendiente ?? pedirMv(c, peticion?.signal)));
  } catch (e) {
    if (cancion.value?.clave === c.clave) fallo(e);
  }
}

// Deja lista la canción que viene: su videoclip de Apple si lo tiene, o el video de YouTube ya cargando.
async function precargarSiguiente() {
  if (!cfg.precargar) return;
  const cola = musicKit()?.queue;
  const prox = aCancion(cola?.items?.[(cola?.position ?? -2) + 1]);
  if (!prox || prox.esVideo || prox.clave === precargada) return;
  precargada = prox.clave;
  try {
    if (cfg.preferirApple && (await appleDe(prox))) return;
    const v = await pedirMv(prox);
    if (v && precargada === prox.clave) siguiente.value = { clave: prox.clave, video: v };
  } catch {
    // La precarga es opcional; al llegar la canción se intenta de nuevo.
  }
}

function reanudarSiPause() {
  if (!pausadoPorCarga) return;
  pausadoPorCarga = false;
  musicKit()?.play?.();
}

function pintarAmbiente(v: HTMLVideoElement) {
  const lienzo = elAmbiente.value;
  if (!lienzo || !cfg.luzAmbiente) return;
  pulso += 1;
  if (cfg.bajoConsumo && pulso % 5 !== 0) return;
  try {
    lienzo.getContext("2d")?.drawImage(v, 0, 0, lienzo.width, lienzo.height);
  } catch {
    // Sin cuadro disponible todavía.
  }
}

function sincronizar() {
  inactivo.value = cfg.ocultar > 0 && !panel.value && Date.now() - ultimaActividad > cfg.ocultar * 1000;
  reproduciendo.value = sonando();
  botonesDeCider(cfg.botonesCider === "nunca" || (cfg.botonesCider === "controles" && inactivo.value));

  const item = tienda()?.nowPlayingItem ?? musicKit()?.nowPlayingItem;
  const actual = aCancion(item);
  if (actual?.clave !== cancion.value?.clave) {
    reanudarSiPause();
    cancion.value = actual;
    reintentado = false;
    resultados.value = [];
    apple.value = null;
    consulta.value = actual ? `${actual.artista} ${actual.titulo}` : "";
    inicioCambio = performance.now();
    if (actual?.esVideo) {
      ultimoApple = String(item?.id ?? "");
      peticion?.abort();
      video.value = null;
      estado.value = "apple";
      precargarSiguiente();
    } else {
      soltarApple();
      if (actual) iniciar(actual);
      else estado.value = "inactivo";
    }
    return;
  }

  if (estado.value === "apple") {
    adoptarApple();
    return;
  }

  const v = elActual();
  if (!v || !video.value || v.readyState < 1) return;
  const fin = Number.isFinite(v.duration) && v.duration > 0 ? v.duration : Infinity;
  let objetivo = tiempoAudio() + video.value.offset;
  if (cfg.finalVideo === "repetir" && objetivo >= fin) objetivo %= fin;

  // Fuera del rango del video se queda quieto en el primer o el último cuadro.
  if (objetivo < 0 || objetivo >= fin - 0.2) {
    if (!v.paused) v.pause();
    return;
  }

  if (cfg.pausarAlCargar) {
    if (v.readyState < 3 && (reproduciendo.value || pausadoPorCarga)) {
      esperandoDesde ||= Date.now();
      if (!pausadoPorCarga && Date.now() - esperandoDesde > cfg.graciaMs) {
        pausadoPorCarga = true;
        musicKit()?.pause?.();
      }
    } else {
      esperandoDesde = 0;
      reanudarSiPause();
    }
  }

  if (!reproduciendo.value) {
    if (!v.paused) v.pause();
    if (Math.abs(v.currentTime - objetivo) > 0.3) v.currentTime = objetivo;
    return;
  }
  if (v.paused) v.play().catch(() => {});
  const deriva = v.currentTime - objetivo;
  if (Math.abs(deriva) > cfg.umbralDuroMs / 1000) {
    v.currentTime = objetivo;
    v.playbackRate = 1;
  } else {
    v.playbackRate = Math.abs(deriva) > cfg.umbralSuaveMs / 1000 ? 1 - Math.max(-0.05, Math.min(0.05, deriva * 0.5)) : 1;
  }
  pintarAmbiente(v);
}

function fallaVideo(v: Video) {
  if (v !== video.value) {
    siguiente.value = null;
    return;
  }
  // El enlace directo caduca; se pide uno nuevo una sola vez.
  if (cancion.value && !reintentado) {
    reintentado = true;
    cargarVideo(cancion.value);
  } else {
    estado.value = "error";
    mensaje.value = "El video no se pudo reproducir.";
    avisar(`fallo de reproducción: ${v.videoId}`);
  }
}

async function fijar(cambio: Record<string, unknown>) {
  const c = cancion.value;
  if (!c) return false;
  try {
    const r = await fetch(`${cfg.servicio}/set`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ key: c.clave, artist: c.artista, title: c.titulo, ...cambio }),
    });
    const datos = await r.json().catch(() => null);
    if (!r.ok) throw new Error(datos?.error ?? `HTTP ${r.status}`);
    mensaje.value = "";
    return true;
  } catch (e: any) {
    mensaje.value = String(e?.message ?? e);
    return false;
  }
}

async function aplicar(cambio: Record<string, unknown>) {
  if ((await fijar(cambio)) && cancion.value) cargarVideo(cancion.value);
}

async function desfasar(delta: number) {
  if (!video.value) return;
  const nuevo = Math.round((video.value.offset + delta) * 10) / 10;
  if (await fijar({ offset: nuevo })) video.value.offset = nuevo;
}

async function recortar(delta: number) {
  if (!video.value) return;
  const nuevo = Math.min(0.3, Math.max(0, Math.round((video.value.recorte + delta) * 100) / 100));
  if (await fijar({ crop: nuevo })) video.value.recorte = nuevo;
}

// Los tres botones de Cider sobre la vista (menú, Coverflow, Focus) se ocultan según la preferencia.
function botonesDeCider(ocultos: boolean) {
  elRaiz.value?.closest(".root-layout-container")?.classList.toggle("vc-ocultar-cider", ocultos);
}

async function buscar() {
  const q = consulta.value.trim();
  if (!q) return;
  if (/^https?:\/\//.test(q)) return aplicar({ video: q });
  buscando.value = true;
  try {
    const r = await fetch(`${cfg.servicio}/search?${new URLSearchParams({ q, cookies: cfg.ytCookies })}`);
    const datos = await r.json();
    if (!r.ok) throw new Error(datos?.error ?? `HTTP ${r.status}`);
    resultados.value = datos.results ?? [];
    mensaje.value = resultados.value.length ? "" : "Sin resultados.";
  } catch (e: any) {
    mensaje.value = String(e?.message ?? e);
  } finally {
    buscando.value = false;
  }
}

function cambioEnLista(clave: string) {
  if (cancion.value?.clave === clave) cargarVideo(cancion.value);
}

function duracionTexto(s: number | null) {
  if (s == null) return "";
  return `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
}

function actividad() {
  ultimaActividad = Date.now();
  inactivo.value = false;
}

const fuente = computed(() => (estado.value === "apple" ? "Apple" : estado.value === "listo" ? "YouTube" : "sin video"));

const conVideo = computed(() => estado.value === "listo" || estado.value === "apple");

const variables = computed(() => {
  const v: Record<string, string> = {
    "--vc-letra": `${cfg.letrasTam}px`,
    "--vc-letras-ancho": `${cfg.letrasAncho}%`,
    "--vc-atenuar": String(estado.value === "listo" ? cfg.atenuar : 0),
    "--vc-grano": String(cfg.grano),
    "--vc-filtro": `blur(${cfg.desenfoque}px) saturate(${cfg.saturacion})`,
  };
  if (cfg.estilo === "ryoku" && paleta.value) {
    const p = paleta.value;
    Object.assign(v, { "--vc-acento": p.primary, "--vc-texto": p.onSurface, "--vc-fondo": p.surface, "--vc-panel": p.surfaceContainer });
  } else if (cfg.estilo === "caratula" && cancion.value?.colores.length) {
    const [fondo, texto, acento] = cancion.value.colores;
    Object.assign(v, { "--vc-acento": acento ?? texto ?? "#fff", "--vc-texto": texto ?? "#fff", "--vc-fondo": fondo, "--vc-panel": fondo });
  }
  return v;
});

const clases = computed(() => [
  `vc-d-${cfg.disposicion}`,
  `vc-encaje-${cfg.encaje}`,
  `vc-letras-${cfg.letrasModo}`,
  `vc-pos-${cfg.letrasPos}`,
  `vc-lectura-${cfg.letrasFondo}`,
  `vc-barra-${cfg.barra}`,
  {
    "vc-con-video": conVideo.value,
    "vc-estrecho": estrecho.value,
    "vc-sobre": cfg.disposicion === "completo" || cfg.letrasSitio === "sobre",
    "vc-inactivo": inactivo.value,
    "vc-expandir": cfg.expandir,
    "vc-con-cola": cfg.cola,
    "vc-cola-lado": cfg.colaCompleta === "lado",
    "vc-con-letras": cfg.letras,
    "vc-brillo": cfg.brilloMarco,
    "vc-transparente": cfg.interfazTransparente,
    "vc-sonando": reproduciendo.value,
  },
]);

onErrorCaptured((e) => {
  avisar(`error en la vista: ${e instanceof Error ? e.message : String(e)}`);
});

onMounted(() => {
  vistasAbiertas.value += 1;
  fetch(`${cfg.servicio}/palette`)
    .then((r) => (r.ok ? r.json() : null))
    .then((p) => (paleta.value = p))
    .catch(() => {});
  observador = new ResizeObserver(([e]) => (estrecho.value = e.contentRect.width < 1100));
  if (elRaiz.value) observador.observe(elRaiz.value);
  sincronizar();
  reloj = window.setInterval(sincronizar, 200);
});

onUnmounted(() => {
  vistasAbiertas.value -= 1;
  botonesDeCider(false);
  window.clearInterval(reloj);
  observador?.disconnect();
  peticion?.abort();
  reanudarSiPause();
  soltarApple();
});

watch(
  () => cfg.calidad,
  () => cancion.value && cargarVideo(cancion.value),
);
</script>

<template>
  <div ref="elRaiz" class="vc-raiz" :class="clases" :style="variables" @mousemove="actividad" @mousedown="actividad">
    <div class="vc-fondo" :style="{ backgroundImage: cancion?.caratula ? `url(${cancion.caratula})` : 'none' }"></div>
    <canvas v-show="estado === 'listo' && cfg.luzAmbiente" ref="elAmbiente" class="vc-ambiente" width="32" height="18"></canvas>

    <div class="vc-escena">
      <div class="vc-principal">
        <div class="vc-marco">
          <video
            v-for="v in videos"
            :key="v.streamUrl"
            :ref="(el) => registrar(v, el)"
            class="vc-video"
            :class="{ 'vc-en-espera': v !== video }"
            :style="{ scale: v.recorte ? String(1 / (1 - 2 * v.recorte)) : undefined }"
            :src="v.streamUrl"
            muted
            playsinline
            preload="auto"
            @error="fallaVideo(v)"
          ></video>
          <div ref="elApple" class="vc-apple"></div>
          <div class="vc-velo"></div>
          <div class="vc-grano" :style="{ backgroundImage: GRANO }"></div>

          <div v-if="!conVideo && cancion" class="vc-centro">
            <div class="vc-portada">
              <div v-if="cfg.vinilo" class="vc-vinilo"></div>
              <img v-if="cancion.caratula" :src="cancion.caratula" alt="" />
            </div>
            <div class="vc-titulo">{{ cancion.titulo }}</div>
            <div class="vc-artista">{{ cancion.artista }}</div>
            <div v-if="estado === 'buscando'" class="vc-nota">Buscando videoclip…</div>
            <div v-else-if="estado === 'error'" class="vc-nota vc-error">{{ mensaje }}</div>
            <button v-if="apple && estado !== 'buscando'" class="vc-apple-boton" @click="verEnApple">Ver videoclip de Apple</button>
          </div>

          <div v-if="conVideo && cancion && (cfg.rotulo !== 'no' || cfg.caratulaMini)" class="vc-rotulo" :class="`vc-rotulo-${cfg.rotulo}`">
            <img v-if="cfg.caratulaMini && cancion.caratula" :src="cancion.caratula" alt="" />
            <div v-if="cfg.rotulo !== 'no'">
              <div class="vc-rotulo-titulo">{{ cancion.titulo }}</div>
              <div class="vc-rotulo-artista">{{ cancion.artista }}</div>
            </div>
          </div>
        </div>

        <div v-if="cfg.letras" class="vc-letras">
          <cider-immersive-lyric-view />
        </div>
      </div>

      <aside v-if="cfg.cola" class="vc-cola">
        <cider-amqueue />
      </aside>
    </div>

    <div class="vc-botones">
      <button
        :title="cfg.disposicion === 'tarjetas' ? 'Cambiar a pantalla completa' : 'Cambiar a tarjetas'"
        @click="cfg.disposicion = cfg.disposicion === 'tarjetas' ? 'completo' : 'tarjetas'"
      >
        {{ cfg.disposicion === "tarjetas" ? "Completa" : "Tarjetas" }}
      </button>
      <button :class="{ activo: cfg.letras }" title="Letras" @click="cfg.letras = !cfg.letras">Letras</button>
      <button :class="{ activo: cfg.cola }" title="Cola de reproducción" @click="cfg.cola = !cfg.cola">Cola</button>
      <button class="vc-boton-mv" :class="{ activo: panel }" :title="`Maku · fuente: ${fuente}`" @click="panel = !panel">
        <span>幕</span>
        <svg v-if="estado === 'apple'" viewBox="0 0 24 24" aria-hidden="true">
          <path
            d="M12.152 6.896c-.948 0-2.415-1.078-3.96-1.04-2.04.027-3.91 1.183-4.961 3.014-2.117 3.675-.546 9.103 1.519 12.09 1.013 1.454 2.208 3.09 3.792 3.039 1.52-.065 2.09-.987 3.935-.987 1.831 0 2.35.987 3.96.948 1.637-.026 2.676-1.48 3.676-2.948 1.156-1.688 1.636-3.325 1.662-3.415-.039-.013-3.182-1.221-3.22-4.857-.026-3.04 2.48-4.494 2.597-4.559-1.429-2.09-3.623-2.324-4.39-2.376-2-.156-3.675 1.09-4.61 1.09zM15.53 3.83c.843-1.012 1.4-2.427 1.245-3.83-1.207.052-2.662.805-3.532 1.818-.78.896-1.454 2.338-1.273 3.714 1.338.104 2.715-.688 3.559-1.701"
          />
        </svg>
        <svg v-else-if="estado === 'listo'" viewBox="0 0 24 24" aria-hidden="true">
          <path
            d="M23.498 6.186a3.016 3.016 0 0 0-2.122-2.136C19.505 3.545 12 3.545 12 3.545s-7.505 0-9.377.505A3.017 3.017 0 0 0 .502 6.186C0 8.07 0 12 0 12s0 3.93.502 5.814a3.016 3.016 0 0 0 2.122 2.136c1.871.505 9.376.505 9.376.505s7.505 0 9.377-.505a3.015 3.015 0 0 0 2.122-2.136C24 15.93 24 12 24 12s0-3.93-.502-5.814zM9.545 15.568V8.432L15.818 12l-6.273 3.568z"
          />
        </svg>
      </button>
    </div>

    <div v-if="panel" class="vc-panel">
      <div class="vc-pestanas">
        <button :class="{ activo: pestana === 'cancion' }" @click="pestana = 'cancion'">Canción</button>
        <button :class="{ activo: pestana === 'ajustes' }" @click="pestana = 'ajustes'">Ajustes</button>
        <button :class="{ activo: pestana === 'videos' }" @click="pestana = 'videos'">Mis videos</button>
        <button class="vc-plano" @click="panel = false">✕</button>
      </div>

      <template v-if="pestana === 'cancion'">
        <div v-if="video" class="vc-actual">
          <div class="vc-actual-titulo">{{ video.videoTitle }}</div>
          <div class="vc-tenue">{{ video.channel }} · {{ video.origen === "manual" ? "elegido a mano" : "automático" }}</div>
          <div v-if="video.descartados" class="vc-tenue">
            El primer resultado tenía restricción de edad en YouTube y se usó otro. Para ver los restringidos, indica tu
            navegador en Ajustes → Búsqueda → Cookies de YouTube.
          </div>
          <div class="vc-fila">
            <span>Desfase {{ video.offset.toFixed(1) }} s</span>
            <span class="vc-grupo">
              <button @click="desfasar(-1)">−1</button>
              <button @click="desfasar(-0.1)">−0,1</button>
              <button @click="desfasar(0.1)">+0,1</button>
              <button @click="desfasar(1)">+1</button>
            </span>
          </div>
          <div class="vc-fila">
            <span title="Amplía el video para quitar franjas negras">Recorte {{ Math.round(video.recorte * 100) }} %</span>
            <span class="vc-grupo">
              <button @click="recortar(-0.01)">−</button>
              <button @click="recortar(0.01)">+</button>
              <button @click="recortar(-1)">Quitar</button>
            </span>
          </div>
        </div>
        <div v-else-if="estado === 'apple'" class="vc-tenue">Estás viendo el videoclip de Apple Music; no se busca en YouTube.</div>
        <div v-else-if="estado === 'sin-video'" class="vc-tenue">Esta canción no tiene video.</div>
        <div v-else-if="estado === 'buscando'" class="vc-tenue">Buscando…</div>

        <form class="vc-buscar" @submit.prevent="buscar">
          <input type="text" v-model="consulta" placeholder="Buscar o pegar un enlace de YouTube" />
          <button type="submit" :disabled="buscando">{{ buscando ? "…" : "Buscar" }}</button>
        </form>

        <div v-if="resultados.length" class="vc-resultados">
          <button v-for="r in resultados" :key="r.id" class="vc-resultado" @click="aplicar({ video: r.id })">
            <img :src="`https://i.ytimg.com/vi/${r.id}/mqdefault.jpg`" alt="" />
            <span>
              <span class="vc-resultado-titulo">{{ r.title }}</span>
              <span class="vc-tenue">{{ r.channel }} · {{ duracionTexto(r.duration) }}</span>
            </span>
          </button>
        </div>

        <button v-if="apple" @click="verEnApple">Ver videoclip de Apple: {{ apple.nombre }}</button>
        <div class="vc-fila">
          <button @click="aplicar({ auto: true })">Volver a buscar solo</button>
          <button @click="aplicar({ none: true })">Sin video en esta canción</button>
        </div>
        <div v-if="mensaje" class="vc-error">{{ mensaje }}</div>
      </template>

      <Opciones v-else-if="pestana === 'ajustes'" />
      <ListaVideos v-else @cambio="cambioEnLista" />
    </div>

    <div v-if="cfg.barra !== 'ninguna'" class="vc-reproductor">
      <cider-immersive-solarium-player v-if="cfg.barra === 'solarium'" />
      <cider-mojave-player v-else />
    </div>
  </div>
</template>

<style lang="scss">
.vc-raiz {
  --vc-acento: var(--keyColor, #fa586a);
  --vc-texto: #fff;
  --vc-fondo: #000;
  --vc-panel: #1c1c1e;
  --vc-cristal: color-mix(in srgb, var(--vc-texto) 9%, transparent);
  --vc-borde: color-mix(in srgb, var(--vc-texto) 16%, transparent);
  --vc-hueco-barra: 104px;

  position: absolute;
  inset: 0;
  overflow: hidden;
  background: var(--vc-fondo);
  color: var(--vc-texto);

  // Cider deja arrastrar la ventana desde la vista inmersiva; lo interactivo se excluye.
  button,
  .vc-botones,
  .vc-panel,
  .vc-cola,
  .vc-letras,
  .vc-reproductor {
    -webkit-app-region: no-drag;
  }

  &.vc-barra-ninguna,
  &.vc-inactivo.vc-expandir {
    --vc-hueco-barra: 22px;
  }

  &.vc-transparente {
    --vc-cristal: transparent;
  }

  &.vc-inactivo {
    cursor: none;

    .vc-botones,
    .vc-reproductor {
      opacity: 0;
      pointer-events: none;
    }
  }

  .vc-fondo,
  .vc-ambiente {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }

  .vc-fondo {
    background-size: cover;
    background-position: center;
    filter: blur(70px) saturate(1.4) brightness(0.6);
    transform: scale(1.25);
  }

  .vc-ambiente {
    filter: blur(80px) saturate(1.6) brightness(0.75);
    transform: scale(1.3);
  }

  .vc-escena {
    position: absolute;
    inset: 42px 22px var(--vc-hueco-barra) 22px;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 18px;
    transition: inset 0.35s ease;
  }

  &.vc-con-cola .vc-escena {
    grid-template-columns: minmax(0, 1fr) minmax(280px, 26%);
  }

  .vc-principal {
    position: relative;
    display: grid;
    grid-template-rows: minmax(0, 1fr) auto;
    gap: 14px;
    min-height: 0;
  }

  .vc-marco {
    position: relative;
    overflow: hidden;
    min-height: 0;
    border-radius: 22px;
    background: #000;
    border: 1px solid var(--vc-borde);
  }

  &.vc-brillo.vc-con-video .vc-marco {
    box-shadow: 0 0 70px color-mix(in srgb, var(--vc-acento) 28%, transparent), 0 18px 50px rgba(0, 0, 0, 0.45);
  }

  &:not(.vc-con-video) .vc-marco {
    background: transparent;
    border-color: transparent;
  }

  .vc-video,
  .vc-velo,
  .vc-grano {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }

  .vc-apple {
    position: absolute;
    inset: 0;

    &:empty {
      display: none;
    }

    video {
      width: 100%;
      height: 100%;
      object-fit: cover;
    }
  }

  .vc-video {
    object-fit: cover;
    filter: var(--vc-filtro);
  }

  .vc-en-espera {
    visibility: hidden;
  }

  &.vc-encaje-completo .vc-video,
  &.vc-encaje-completo .vc-apple video {
    object-fit: contain;
  }

  .vc-apple-boton {
    margin-top: 12px;
    padding: 8px 16px;
    border: 0;
    border-radius: 999px;
    font-weight: 600;
    cursor: pointer;
    color: var(--vc-fondo);
    background: var(--vc-acento);
  }

  &.vc-encaje-ancho .vc-video {
    height: auto;
    top: 50%;
    bottom: auto;
    transform: translateY(-50%);
  }

  .vc-velo {
    background: #000;
    opacity: var(--vc-atenuar);
    pointer-events: none;
    transition: opacity 0.4s ease;
  }

  .vc-grano {
    opacity: var(--vc-grano);
    mix-blend-mode: overlay;
    pointer-events: none;
  }

  .vc-centro {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    text-align: center;
    padding: 24px;
  }

  // La carátula encoge antes que el texto: en marcos bajos el aviso no debe quedar cortado.
  .vc-portada {
    position: relative;
    flex: 0 1 min(44vh, 40vw);
    min-height: 90px;
    aspect-ratio: 1;
    margin-bottom: 14px;

    img {
      position: relative;
      width: 100%;
      height: 100%;
      border-radius: 14px;
      box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
    }
  }

  .vc-vinilo {
    position: absolute;
    top: 4%;
    left: 34%;
    width: 92%;
    aspect-ratio: 1;
    border-radius: 50%;
    background:
      radial-gradient(circle, var(--vc-acento) 0 11%, #111 11.5% 13%, transparent 13.5%),
      repeating-radial-gradient(circle, #151515 0 2px, #060606 2px 4px);
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
    animation: vc-girar 6s linear infinite;
    animation-play-state: paused;
  }

  &.vc-sonando .vc-vinilo {
    animation-play-state: running;
  }

  .vc-titulo {
    font-size: 26px;
    font-weight: 700;
  }

  .vc-artista {
    font-size: 17px;
    opacity: 0.8;
  }

  .vc-nota {
    flex: none;
    max-width: 640px;
    margin-top: 8px;
    font-size: 13px;
    line-height: 1.4;
    opacity: 0.7;
  }

  .vc-titulo,
  .vc-artista {
    flex: none;
  }

  .vc-error {
    color: #ff8a80;
    opacity: 1;
  }

  .vc-rotulo {
    position: absolute;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 18px 22px;
    text-shadow: 0 2px 12px rgba(0, 0, 0, 0.85);

    img {
      width: 56px;
      height: 56px;
      border-radius: 8px;
      box-shadow: 0 6px 20px rgba(0, 0, 0, 0.5);
    }
  }

  .vc-rotulo-no,
  .vc-rotulo-arriba-izquierda {
    top: 0;
    left: 0;
  }

  .vc-rotulo-arriba-centro {
    top: 0;
    left: 50%;
    transform: translateX(-50%);
    text-align: center;
  }

  .vc-rotulo-abajo-izquierda {
    bottom: 0;
    left: 0;
  }

  .vc-rotulo-titulo {
    font-size: 20px;
    font-weight: 700;
  }

  .vc-rotulo-artista {
    font-size: 14px;
    opacity: 0.85;
  }

  .vc-letras {
    overflow: hidden;
    border-radius: 22px;
    background: var(--vc-cristal);
    border: 1px solid var(--vc-borde);
    backdrop-filter: blur(24px);
    height: 38vh;
    transition: height 0.3s ease;

    .lyric-view {
      padding: 14px 22px !important;
    }

    .lyric-line {
      --fontSize: var(--vc-letra) !important;
      --fontSizeBG: calc(var(--vc-letra) * 0.6) !important;
    }
  }

  &.vc-letras-compacta .vc-letras {
    height: calc(var(--vc-letra) * 4.6 + 28px);
  }

  &.vc-letras-linea .vc-letras {
    height: calc(var(--vc-letra) * 1.7 + 28px);
  }

  &.vc-lectura-sombra .vc-letras {
    text-shadow: 0 2px 10px rgba(0, 0, 0, 0.85);
  }

  &.vc-lectura-panel .vc-letras {
    background: color-mix(in srgb, var(--vc-fondo) 62%, transparent);
  }

  &.vc-lectura-ninguno .vc-letras,
  &.vc-transparente .vc-letras {
    background: transparent;
    border-color: transparent;
    backdrop-filter: none;
  }

  .vc-cola {
    overflow: hidden auto;
    min-height: 0;
    border-radius: 22px;
    background: var(--vc-cristal);
    border: 1px solid var(--vc-borde);
    backdrop-filter: blur(24px);
  }

  &.vc-transparente .vc-cola {
    border-color: transparent;
    backdrop-filter: none;
  }

  // Pantalla completa: el video ocupa todo y lo demás flota encima.
  &.vc-d-completo {
    --vc-ancho-cola: min(26%, 400px);

    .vc-escena {
      display: block;
      inset: 0;
    }

    .vc-marco {
      position: absolute;
      inset: 0;
      border: 0;
      border-radius: 0;
      box-shadow: none;
    }

    // Sin posición propia: el video y las letras se colocan respecto a toda la ventana.
    .vc-principal {
      display: block;
      position: static;
    }

    // La cola se acopla al lado: el video cede ese ancho en vez de quedar tapado.
    &.vc-con-cola.vc-cola-lado .vc-marco {
      right: calc(var(--vc-ancho-cola) + 44px);
      border-radius: 0 22px 22px 0;
    }

    &.vc-con-cola .vc-letras {
      max-width: calc(100% - var(--vc-ancho-cola) - 88px);
    }

    .vc-cola {
      position: absolute;
      top: 42px;
      right: 22px;
      bottom: var(--vc-hueco-barra);
      width: var(--vc-ancho-cola);
      z-index: 3;
      background: color-mix(in srgb, var(--vc-fondo) 50%, transparent);
      backdrop-filter: blur(30px) saturate(1.3);
      box-shadow: 0 18px 50px rgba(0, 0, 0, 0.35);
      transition: bottom 0.35s ease;
    }

    .vc-rotulo-abajo-izquierda {
      bottom: var(--vc-hueco-barra);
    }

    &.vc-con-letras.vc-pos-izquierda .vc-rotulo-abajo-izquierda,
    &.vc-con-letras.vc-pos-abajo .vc-rotulo-abajo-izquierda {
      bottom: auto;
      top: 0;
    }
  }

  // Letras flotando sobre el video: siempre en pantalla completa, opcional en tarjetas.
  &.vc-sobre {
    --vc-letras-abajo: 18px;

    &.vc-d-completo {
      --vc-letras-abajo: var(--vc-hueco-barra);
    }

    .vc-letras {
      position: absolute;
      z-index: 2;
      left: 22px;
      bottom: var(--vc-letras-abajo);
      width: var(--vc-letras-ancho);
      height: 44%;
      transition: bottom 0.35s ease, height 0.3s ease;
    }

    &.vc-letras-compacta .vc-letras {
      height: calc(var(--vc-letra) * 4.6 + 28px);
    }

    &.vc-letras-linea .vc-letras {
      height: calc(var(--vc-letra) * 1.7 + 28px);
    }

    &.vc-pos-centro .vc-letras {
      left: 50%;
      transform: translateX(-50%);
      text-align: center;
    }

    &.vc-pos-derecha .vc-letras {
      left: auto;
      right: 22px;
      text-align: right;
    }

    &.vc-pos-abajo .vc-letras {
      left: 22px;
      right: 22px;
      width: auto;
      text-align: center;
    }

    &.vc-d-completo.vc-con-cola.vc-pos-derecha .vc-letras,
    &.vc-d-completo.vc-con-cola.vc-pos-abajo .vc-letras {
      right: calc(var(--vc-ancho-cola) + 44px);
    }

    &.vc-d-completo.vc-con-cola.vc-pos-centro .vc-letras {
      left: calc((100% - var(--vc-ancho-cola) - 44px) / 2);
    }

    &:not(.vc-lectura-panel) .vc-letras {
      background: transparent;
      border-color: transparent;
      backdrop-filter: none;
    }
  }

  // Ventana estrecha o vertical: una sola columna, cola debajo y botones sobre la barra.
  &.vc-estrecho {
    --vc-hueco-barra: 150px;
    --vc-ancho-cola: 100%;

    &.vc-barra-ninguna,
    &.vc-inactivo.vc-expandir {
      --vc-hueco-barra: 22px;
    }

    .vc-escena {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: minmax(0, 1fr);
    }

    &.vc-con-cola .vc-escena {
      grid-template-rows: minmax(0, 1fr) minmax(120px, 28%);
    }

    // En vertical el video conserva su proporción y las letras se quedan con el resto del alto.
    &.vc-d-tarjetas.vc-con-letras:not(.vc-sobre) {
      .vc-principal {
        grid-template-rows: auto minmax(0, 1fr);
      }

      .vc-marco {
        aspect-ratio: 16 / 9;
      }

      .vc-letras {
        height: auto;
      }
    }

    &.vc-d-completo .vc-cola {
      top: auto;
      left: 22px;
      width: auto;
      height: 28%;
    }

    &.vc-d-completo.vc-con-cola .vc-letras {
      max-width: none;
      bottom: calc(var(--vc-hueco-barra) + 28% + 14px);
    }

    // Acoplada en vertical, la cola va debajo: el video cede alto en vez de ancho.
    &.vc-d-completo.vc-con-cola.vc-cola-lado .vc-marco {
      right: 0;
      bottom: calc(var(--vc-hueco-barra) + 28% + 14px);
      border-radius: 0 0 22px 22px;
    }

    &.vc-d-completo.vc-con-cola.vc-cola-lado .vc-letras {
      bottom: calc(var(--vc-hueco-barra) + 28% + 28px);
    }

    &.vc-sobre .vc-letras {
      width: auto;
      right: 22px;
    }

    .vc-botones {
      bottom: 100px;
      left: 0;
      right: 0;
      justify-content: center;
    }

    .vc-panel {
      left: 12px;
      right: 12px;
      width: auto;
      bottom: 140px;
      max-height: calc(100% - 190px);
    }

    .vc-reproductor {
      transform-origin: center bottom;
      transform: scale(var(--vc-escala-barra, 1));
    }
  }

  // Abajo a la derecha: la franja superior es de Cider para arrastrar la ventana y tapa los clics.
  .vc-botones {
    position: absolute;
    bottom: 32px;
    right: 22px;
    z-index: 5;
    display: flex;
    gap: 6px;
    transition: opacity 0.3s ease;

    button {
      padding: 6px 12px;
      border: 0;
      border-radius: 999px;
      font-weight: 700;
      font-size: 12px;
      cursor: pointer;
      color: var(--vc-texto);
      background: rgba(255, 255, 255, 0.14);
      backdrop-filter: blur(12px);
      opacity: 0.6;
      transition: opacity 0.2s ease, background 0.2s ease;

      &:hover {
        opacity: 1;
      }

      &.activo {
        opacity: 1;
        background: var(--vc-acento);
        color: var(--vc-fondo);
      }
    }

    .vc-boton-mv {
      display: inline-flex;
      align-items: center;
      gap: 6px;

      svg {
        width: 15px;
        height: 15px;
        fill: currentColor;
      }
    }
  }

  .vc-panel {
    position: absolute;
    bottom: 76px;
    right: 22px;
    z-index: 6;
    width: 420px;
    max-height: calc(100% - 130px);
    overflow-y: auto;
    display: grid;
    gap: 12px;
    padding: 14px;
    border-radius: 16px;
    font-size: 13px;
    background: color-mix(in srgb, var(--vc-panel) 90%, transparent);
    border: 1px solid var(--vc-borde);
    backdrop-filter: blur(24px);
    box-shadow: 0 18px 50px rgba(0, 0, 0, 0.45);

    button,
    input[type="text"],
    input[type="number"],
    select {
      font: inherit;
      color: var(--vc-texto);
      background: color-mix(in srgb, var(--vc-texto) 10%, transparent);
      border: 1px solid var(--vc-borde);
      border-radius: 8px;
      padding: 6px 10px;
    }

    option {
      color: #111;
    }

    button {
      cursor: pointer;

      &:hover {
        background: color-mix(in srgb, var(--vc-acento) 30%, transparent);
      }
    }

    input[type="range"],
    input[type="checkbox"] {
      accent-color: var(--vc-acento);
    }
  }

  .vc-pestanas {
    display: flex;
    gap: 6px;

    button.activo {
      background: var(--vc-acento);
      color: var(--vc-fondo);
      font-weight: 600;
    }

    .vc-plano {
      margin-left: auto;
      border: 0;
      background: none;
    }
  }

  .vc-fila {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .vc-actual {
    display: grid;
    gap: 6px;
  }

  .vc-actual-titulo {
    font-weight: 600;
  }

  .vc-tenue {
    opacity: 0.65;
    font-size: 12px;
  }

  .vc-grupo {
    display: flex;
    gap: 4px;
  }

  .vc-buscar {
    display: flex;
    gap: 6px;

    input {
      flex: 1;
      min-width: 0;
    }
  }

  .vc-resultados {
    display: grid;
    gap: 6px;
  }

  .vc-resultado {
    display: grid;
    grid-template-columns: 96px 1fr;
    gap: 10px;
    align-items: center;
    text-align: left;
    padding: 4px !important;

    img {
      width: 96px;
      aspect-ratio: 16 / 9;
      object-fit: cover;
      border-radius: 6px;
    }

    span {
      display: grid;
      gap: 2px;
    }
  }

  .vc-resultado-titulo {
    display: -webkit-box !important;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  // La barra Mojave no trae ancho propio y dentro de una fila flexible se quedaría en cero.
  .vc-reproductor > cider-mojave-player {
    width: min(1100px, calc(100% - 44px));
  }

  .vc-reproductor {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 20px;
    z-index: 4;
    display: flex;
    justify-content: center;
    transition: opacity 0.3s ease;
  }
}

.root-layout-container.vc-ocultar-cider > .q-pa-md > button.q-btn {
  opacity: 0;
  pointer-events: none;
  transition: opacity 0.3s ease;
}

@keyframes vc-girar {
  to {
    transform: rotate(360deg);
  }
}
</style>
