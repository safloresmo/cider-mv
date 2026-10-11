<script setup lang="ts">
import { saveConfig } from "@ciderapp/pluginkit";
import { useConfig } from "../main";

type Campo = {
  clave: string;
  titulo: string;
  ayuda?: string;
  tipo: "lista" | "casilla" | "rango" | "numero" | "texto";
  opciones?: [string | number, string][];
  min?: number;
  max?: number;
  paso?: number;
  ejemplo?: string;
};

const cfg = useConfig() as Record<string, any>;

const secciones: { titulo: string; campos: Campo[] }[] = [
  {
    titulo: "Aspecto",
    campos: [
      { clave: "disposicion", titulo: "Diseño", ayuda: "Tarjetas enmarca el video; pantalla completa lo usa de fondo.", tipo: "lista", opciones: [["tarjetas", "Tarjetas"], ["completo", "Pantalla completa"]] },
      { clave: "estilo", titulo: "Colores", tipo: "lista", opciones: [["ryoku", "Ryoku"], ["caratula", "Carátula"], ["cider", "Cider"]] },
      { clave: "luzAmbiente", titulo: "Luz ambiente", ayuda: "El fondo toma los colores del video.", tipo: "casilla" },
      { clave: "bajoConsumo", titulo: "Luz ambiente de bajo consumo", ayuda: "Actualiza el fondo con menos frecuencia.", tipo: "casilla" },
      { clave: "brilloMarco", titulo: "Brillo del marco", ayuda: "Halo alrededor del video en el diseño de tarjetas.", tipo: "casilla" },
      { clave: "interfazTransparente", titulo: "Interfaz transparente", ayuda: "Tarjetas sin fondo translúcido.", tipo: "casilla" },
      { clave: "vinilo", titulo: "Disco de vinilo tras la carátula", ayuda: "Solo cuando la canción no tiene video.", tipo: "casilla" },
      { clave: "barra", titulo: "Barra de controles", tipo: "lista", opciones: [["solarium", "Solarium"], ["mojave", "Mojave"], ["ninguna", "Ninguna"]] },
      { clave: "ocultar", titulo: "Ocultar controles", ayuda: "Tiempo sin mover el ratón.", tipo: "lista", opciones: [[0, "Nunca"], [2, "2 s"], [4, "4 s"], [8, "8 s"]] },
      { clave: "botonesCider", titulo: "Botones de Cider", ayuda: "Los tres de arriba a la izquierda (menú, Coverflow, Focus). El menú es donde se cambia de diseño.", tipo: "lista", opciones: [["siempre", "Siempre visibles"], ["controles", "Ocultos con los controles"], ["nunca", "Siempre ocultos"]] },
      { clave: "expandir", titulo: "Expandir al ocultar controles", ayuda: "El video y las letras ocupan el hueco de la barra.", tipo: "casilla" },
      { clave: "rotulo", titulo: "Título y artista", tipo: "lista", opciones: [["no", "No mostrar"], ["arriba-izquierda", "Arriba a la izquierda"], ["arriba-centro", "Arriba al centro"], ["abajo-izquierda", "Abajo a la izquierda"]] },
      { clave: "caratulaMini", titulo: "Carátula pequeña", ayuda: "Miniatura de la portada junto al título.", tipo: "casilla" },
      { clave: "cola", titulo: "Cola de reproducción", tipo: "casilla" },
      { clave: "colaCompleta", titulo: "Cola en pantalla completa", ayuda: "Flotando sobre el video o acoplada a su lado.", tipo: "lista", opciones: [["sobre", "Sobre el video"], ["lado", "Al lado del video"]] },
    ],
  },
  {
    titulo: "Video",
    campos: [
      { clave: "encaje", titulo: "Encaje", tipo: "lista", opciones: [["rellenar", "Rellenar (recorta)"], ["completo", "Completo (con franjas)"], ["ancho", "Ajustar al ancho"]] },
      { clave: "calidad", titulo: "Calidad", tipo: "lista", opciones: [["2160", "2160p"], ["1440", "1440p"], ["1080", "1080p"], ["720", "720p"], ["480", "480p"]] },
      { clave: "atenuar", titulo: "Oscurecer", ayuda: "Velo oscuro sobre el video.", tipo: "rango", min: 0, max: 0.8, paso: 0.05 },
      { clave: "desenfoque", titulo: "Desenfoque", tipo: "rango", min: 0, max: 20, paso: 1 },
      { clave: "saturacion", titulo: "Saturación", tipo: "rango", min: 0, max: 2, paso: 0.05 },
      { clave: "grano", titulo: "Grano", tipo: "rango", min: 0, max: 0.25, paso: 0.01 },
      { clave: "finalVideo", titulo: "Si el video acaba antes", tipo: "lista", opciones: [["congelar", "Mantener el último cuadro"], ["repetir", "Repetir el video"]] },
      { clave: "precargar", titulo: "Precargar la siguiente", ayuda: "Busca el video de la canción que viene.", tipo: "casilla" },
    ],
  },
  {
    titulo: "Letras",
    campos: [
      { clave: "letras", titulo: "Mostrar letras", tipo: "casilla" },
      { clave: "letrasModo", titulo: "Modo", tipo: "lista", opciones: [["completa", "Completa"], ["compacta", "Compacta"], ["linea", "Solo la línea actual"]] },
      { clave: "letrasSitio", titulo: "Lugar en el diseño de tarjetas", ayuda: "En pantalla completa van siempre sobre el video.", tipo: "lista", opciones: [["tarjeta", "En su propia tarjeta"], ["sobre", "Sobre el video"]] },
      { clave: "letrasPos", titulo: "Posición sobre el video", tipo: "lista", opciones: [["izquierda", "Izquierda"], ["centro", "Centro"], ["derecha", "Derecha"], ["abajo", "Franja abajo"]] },
      { clave: "letrasAncho", titulo: "Ancho sobre el video", tipo: "rango", min: 25, max: 100, paso: 1 },
      { clave: "letrasTam", titulo: "Tamaño", tipo: "rango", min: 16, max: 64, paso: 1 },
      { clave: "letrasFondo", titulo: "Fondo de lectura", tipo: "lista", opciones: [["sombra", "Sombra"], ["panel", "Panel translúcido"], ["ninguno", "Ninguno"]] },
      { clave: "letrasAnimacion", titulo: "Animación", ayuda: "Cuanto más sencilla, menos trabajo para el equipo. Mínima ilumina palabra por palabra, sin relleno ni brillo.", tipo: "lista", opciones: [["completa", "Completa"], ["ligera", "Ligera"], ["minima", "Mínima"]] },
    ],
  },
  {
    titulo: "Búsqueda",
    campos: [
      { clave: "preferirApple", titulo: "Preferir el videoclip de Apple", ayuda: "Si la canción tiene videoclip en Apple Music, lo reproduce en su lugar (suena el audio del video).", tipo: "casilla" },
      { clave: "busTipo", titulo: "Tipo preferido en YouTube", tipo: "lista", opciones: [["oficial", "Videoclip oficial"], ["lyric", "Lyric video"], ["cualquiera", "Cualquiera"]] },
      { clave: "busExigencia", titulo: "Exigencia", ayuda: "Más alta: menos videos pero más aciertos.", tipo: "lista", opciones: [["baja", "Baja"], ["media", "Media"], ["alta", "Alta"]] },
      { clave: "busExcluir", titulo: "Palabras excluidas", ayuda: "Separadas por comas. Afecta a búsquedas nuevas.", tipo: "texto", ejemplo: "live, cover, reaction" },
      { clave: "ytCookies", titulo: "Cookies de YouTube", ayuda: "Navegador con tu sesión de YouTube, para ver videos con restricción de edad: firefox, chrome, chromium, brave, vivaldi o chromium:/ruta/del/perfil.", tipo: "texto", ejemplo: "firefox" },
      { clave: "prepararFuera", titulo: "Preparar videos con la vista cerrada", ayuda: "Abrir la vista es inmediato, pero consulta a YouTube por cada canción que escuchas y puede provocar que te bloquee un tiempo.", tipo: "casilla" },
    ],
  },
  {
    titulo: "Sincronía avanzada",
    campos: [
      { clave: "pausarAlCargar", titulo: "Pausar Cider mientras carga el video", ayuda: "Evita que el audio se adelante al video.", tipo: "casilla" },
      { clave: "graciaMs", titulo: "Espera antes de pausar", ayuda: "Milisegundos.", tipo: "numero", min: 0, max: 5000 },
      { clave: "umbralDuroMs", titulo: "Umbral de salto", ayuda: "Desfase en ms que fuerza un salto exacto.", tipo: "numero", min: 40, max: 2000 },
      { clave: "umbralSuaveMs", titulo: "Umbral de ajuste suave", ayuda: "Desfase en ms que ajusta la velocidad.", tipo: "numero", min: 5, max: 500 },
    ],
  },
];

function texto(campo: Campo) {
  const v = cfg[campo.clave];
  if (campo.clave === "atenuar" || campo.clave === "grano") return `${Math.round(v * 100)}%`;
  if (campo.clave === "saturacion") return `${Math.round(v * 100)}%`;
  if (campo.clave === "letrasAncho") return `${v}%`;
  return `${v}${campo.clave === "desenfoque" || campo.clave === "letrasTam" ? " px" : ""}`;
}

watch(cfg, () => saveConfig(), { deep: true });
</script>

<template>
  <div class="vc-opciones">
    <section v-for="s in secciones" :key="s.titulo">
      <h4>{{ s.titulo }}</h4>
      <label v-for="c in s.campos" :key="c.clave" :class="{ 'vc-casilla': c.tipo === 'casilla' }">
        <span class="vc-campo">
          <strong>{{ c.titulo }}</strong>
          <small v-if="c.ayuda">{{ c.ayuda }}</small>
        </span>
        <select v-if="c.tipo === 'lista'" v-model="cfg[c.clave]">
          <option v-for="o in c.opciones" :key="o[0]" :value="o[0]">{{ o[1] }}</option>
        </select>
        <input v-else-if="c.tipo === 'casilla'" type="checkbox" v-model="cfg[c.clave]" />
        <span v-else-if="c.tipo === 'rango'" class="vc-rango">
          <input type="range" :min="c.min" :max="c.max" :step="c.paso" v-model.number="cfg[c.clave]" />
          <em>{{ texto(c) }}</em>
        </span>
        <input v-else-if="c.tipo === 'numero'" type="number" :min="c.min" :max="c.max" v-model.number.lazy="cfg[c.clave]" />
        <input v-else type="text" v-model.lazy="cfg[c.clave]" :placeholder="c.ejemplo" />
      </label>
    </section>
  </div>
</template>

<style lang="scss">
.vc-opciones {
  display: grid;
  gap: 12px;

  section {
    display: grid;
    gap: 10px;
    padding: 12px 14px;
    border-radius: 12px;
    background: rgba(127, 127, 127, 0.1);
    border: 1px solid rgba(127, 127, 127, 0.18);
  }

  h4 {
    margin: 0 0 2px;
    font-size: 14px;
    font-weight: 600;
  }

  label {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
  }

  .vc-campo {
    display: grid;
    gap: 1px;
    min-width: 0;

    strong {
      font-weight: 600;
    }

    small {
      opacity: 0.6;
      font-size: 11px;
    }
  }

  select,
  input[type="text"],
  input[type="number"] {
    width: 170px;
    flex: none;
  }

  .vc-rango {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;

    input {
      width: 120px;
    }

    em {
      width: 42px;
      font-style: normal;
      font-size: 12px;
      text-align: right;
      opacity: 0.75;
    }
  }
}
</style>
