<script setup lang="ts">
import Opciones from "./Opciones.vue";
import ListaVideos from "./ListaVideos.vue";
import { useConfig } from "../main";

const cfg = useConfig();
</script>

<template>
  <div class="vc-ajustes">
    <h2>Maku Clips</h2>
    <p>
      El video de cada canción se busca solo. Para cambiarlo o quitarlo en la canción que suena, usa el botón 幕 de la
      vista inmersiva.
    </p>
    <Opciones />
    <label class="vc-servicio">
      <span>Servicio local</span>
      <input type="text" v-model.lazy="cfg.servicio" />
    </label>
    <h3>Mis videos</h3>
    <ListaVideos />
  </div>
</template>

<style lang="scss">
.vc-ajustes {
  display: grid;
  gap: 14px;
  padding: 16px;
  max-width: 560px;

  .vc-servicio {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;

    input {
      min-width: 220px;
    }
  }
}
</style>
