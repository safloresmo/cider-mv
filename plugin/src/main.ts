import { defineCustomElement, ref } from "vue";
import { definePluginContext, addImmersiveLayout } from "@ciderapp/pluginkit";
import Videoclip from "./components/Videoclip.vue";
import Ajustes from "./components/Ajustes.vue";
import PluginConfig from "./plugin.config";
import { aCancion, appleDe, consultaMv, musicKit } from "./cancion";

// Cuántas vistas inmersivas del plugin hay abiertas; con alguna abierta, ella misma pide los videos.
export const vistasAbiertas = ref(0);

export const CustomElements = {
  "immersive-layout": defineCustomElement(Videoclip, { shadowRoot: false }),
};

const { plugin, setupConfig, customElementName, useCPlugin } = definePluginContext({
  ...PluginConfig,
  CustomElements,
  setup() {
    for (const [nombre, elemento] of Object.entries(CustomElements)) {
      const etiqueta = customElementName(nombre);
      if (!customElements.get(etiqueta)) customElements.define(etiqueta, elemento);
    }
    const ajustes = customElementName("settings");
    if (!customElements.get(ajustes)) {
      customElements.define(ajustes, defineCustomElement(Ajustes, { shadowRoot: false }));
    }
    this.SettingsElement = ajustes;

    addImmersiveLayout({
      name: "Videoclip",
      identifier: "videoclip-layout",
      component: customElementName("immersive-layout"),
      type: "normal",
    });
    // Cider usa otro diseño cuando la ventana es más alta que ancha; sin esto ahí no saldría el plugin.
    addImmersiveLayout({
      name: "Videoclip",
      identifier: "videoclip-layout-portrait",
      component: customElementName("immersive-layout"),
      type: "portrait",
    });
  },
});

export type Estilo = "ryoku" | "caratula" | "cider";

export const cfg = setupConfig({
  servicio: "http://127.0.0.1:3060",
  estilo: <Estilo>"ryoku",
  disposicion: <"completo" | "tarjetas">"tarjetas",
  encaje: <"rellenar" | "completo" | "ancho">"rellenar",
  atenuar: <number>0.2,
  desenfoque: <number>0,
  saturacion: <number>1,
  grano: <number>0.03,
  ocultar: <number>4,
  precargar: <boolean>true,
  calidad: "1080",
  letras: <boolean>true,
  letrasModo: <"completa" | "compacta" | "linea">"compacta",
  letrasPos: <"izquierda" | "centro" | "derecha" | "abajo">"izquierda",
  letrasSitio: <"tarjeta" | "sobre">"tarjeta",
  letrasAncho: <number>46,
  letrasTam: <number>30,
  letrasFondo: <"sombra" | "panel" | "ninguno">"sombra",
  rotulo: <"no" | "arriba-izquierda" | "arriba-centro" | "abajo-izquierda">"no",
  caratulaMini: <boolean>false,
  botonesCider: <"siempre" | "controles" | "nunca">"controles",
  cola: <boolean>true,
  colaCompleta: <"sobre" | "lado">"sobre",
  barra: <"solarium" | "mojave" | "ninguna">"solarium",
  finalVideo: <"congelar" | "repetir">"congelar",
  pausarAlCargar: <boolean>false,
  graciaMs: <number>700,
  umbralDuroMs: <number>350,
  umbralSuaveMs: <number>50,
  brilloMarco: <boolean>true,
  luzAmbiente: <boolean>true,
  bajoConsumo: <boolean>false,
  interfazTransparente: <boolean>false,
  vinilo: <boolean>true,
  expandir: <boolean>true,
  preferirApple: <boolean>true,
  prepararFuera: <boolean>false,
  ytCookies: "",
  busTipo: <"oficial" | "lyric" | "cualquiera">"oficial",
  busExigencia: <"baja" | "media" | "alta">"media",
  busExcluir: "",
});

// Con la vista cerrada, deja resuelto el video de lo que suena para que abrirla sea inmediato.
let ultimaPreparada = "";
window.setInterval(async () => {
  const c = cfg.value;
  if (!c.prepararFuera || vistasAbiertas.value > 0) return;
  const cancion = aCancion(musicKit()?.nowPlayingItem);
  if (!cancion || cancion.esVideo || cancion.clave === ultimaPreparada) return;
  ultimaPreparada = cancion.clave;
  if (c.preferirApple && (await appleDe(cancion))) return;
  fetch(`${c.servicio}/mv?${consultaMv(cancion, c)}`).catch(() => {});
}, 3000);

export function useConfig() {
  return cfg.value;
}

export { setupConfig, customElementName, useCPlugin };

export default plugin;
