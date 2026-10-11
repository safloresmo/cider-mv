<script setup lang="ts">
import { useConfig } from "../main";

type Entrada = { key: string; artista?: string; titulo?: string; videoId?: string; none?: boolean; origen?: string };

const emit = defineEmits<{ cambio: [clave: string] }>();
const cfg = useConfig();
const entradas = ref<Entrada[]>([]);
const filtro = ref("");
const error = ref("");

const visibles = computed(() => {
  const f = filtro.value.trim().toLowerCase();
  if (!f) return entradas.value;
  return entradas.value.filter((e) => `${e.artista ?? ""} ${e.titulo ?? ""}`.toLowerCase().includes(f));
});

async function cargar() {
  try {
    const r = await fetch(`${cfg.servicio}/list`);
    entradas.value = (await r.json()).items ?? [];
    error.value = "";
  } catch {
    error.value = "No se pudo conectar con el servicio cider-mv.";
  }
}

async function fijar(e: Entrada, cambio: Record<string, unknown>) {
  try {
    const r = await fetch(`${cfg.servicio}/set`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ key: e.key, artist: e.artista, title: e.titulo, ...cambio }),
    });
    if (!r.ok) throw new Error((await r.json().catch(() => null))?.error ?? `HTTP ${r.status}`);
    await cargar();
    emit("cambio", e.key);
  } catch (err: any) {
    error.value = String(err?.message ?? err);
  }
}

onMounted(cargar);
defineExpose({ cargar });
</script>

<template>
  <div class="vc-lista">
    <input type="text" v-model="filtro" :placeholder="`Filtrar ${entradas.length} canciones`" />
    <div v-if="error" class="vc-error">{{ error }}</div>
    <div v-for="e in visibles" :key="e.key" class="vc-lista-fila">
      <img v-if="e.videoId" :src="`https://i.ytimg.com/vi/${e.videoId}/mqdefault.jpg`" alt="" />
      <div v-else class="vc-lista-vacio">sin video</div>
      <div class="vc-lista-texto">
        <span>{{ e.titulo || e.key }}</span>
        <span class="vc-tenue">{{ e.artista }} · {{ e.origen === "manual" ? "a mano" : "automático" }}</span>
      </div>
      <div class="vc-lista-acciones">
        <button title="Olvidar y volver a buscar solo" @click="fijar(e, { auto: true })">↻</button>
        <button v-if="!e.none" title="Dejar esta canción sin video" @click="fijar(e, { none: true })">✕</button>
      </div>
    </div>
    <div v-if="!visibles.length && !error" class="vc-tenue">Aún no hay canciones guardadas.</div>
  </div>
</template>

<style lang="scss">
.vc-lista {
  display: grid;
  gap: 6px;

  .vc-lista-fila {
    display: grid;
    grid-template-columns: 72px 1fr auto;
    gap: 10px;
    align-items: center;
  }

  img,
  .vc-lista-vacio {
    width: 72px;
    aspect-ratio: 16 / 9;
    object-fit: cover;
    border-radius: 6px;
  }

  .vc-lista-vacio {
    display: grid;
    place-items: center;
    font-size: 10px;
    opacity: 0.6;
    background: rgba(127, 127, 127, 0.2);
  }

  .vc-lista-texto {
    display: grid;
    min-width: 0;

    span {
      overflow: hidden;
      white-space: nowrap;
      text-overflow: ellipsis;
    }
  }

  .vc-lista-acciones {
    display: flex;
    gap: 4px;
  }

  .vc-tenue {
    opacity: 0.65;
    font-size: 12px;
  }

  .vc-error {
    color: #ff8a80;
  }
}
</style>
