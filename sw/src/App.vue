<script setup lang="ts">
import ChannelStrip from "./components/ChannelStrip.vue";

</script>

<template>
  <div class="flex h-full">
    <div
      v-for="i in 4"
      :key="i"
      class="w-1/4 overflow-hidden"
    >
      <ChannelStrip :channel-index="i" />
    </div>
  </div>
</template>
