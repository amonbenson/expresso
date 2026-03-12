<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { onMounted, onUnmounted, ref } from "vue";

import ChannelStrip from "./components/ChannelStrip.vue";
import type { ExpressionChannelSettings, Settings } from "./types/settings";

// null = no device connected / settings not yet loaded
const channelSettings = ref<[
  ExpressionChannelSettings | null,
  ExpressionChannelSettings | null,
  ExpressionChannelSettings | null,
  ExpressionChannelSettings | null,
]>([null, null, null, null]);

const deviceConnected = ref(false);

function applySettings(settings: Settings): void {
  channelSettings.value = [
    settings.expression.channels[0],
    settings.expression.channels[1],
    settings.expression.channels[2],
    settings.expression.channels[3],
  ];
}

// Tauri event listeners — returned unlisten functions collected for cleanup
type Unlisten = () => void;
const unlisteners: Unlisten[] = [];

interface InitialState {
  connected: boolean;
  settings: Settings | null;
}

onMounted(async () => {
  // Register listeners before querying state to avoid missing events fired
  // between app start and listener registration.
  unlisteners.push(
    await listen<Settings>("settings-loaded", ({ payload }) => {
      applySettings(payload);
      deviceConnected.value = true;
    }),
    await listen("device-connected", () => {
      deviceConnected.value = true;
    }),
    await listen("device-disconnected", () => {
      deviceConnected.value = false;
      channelSettings.value = [null, null, null, null];
    }),
  );

  // Apply whatever state the backend already has (device may have connected
  // before the frontend finished mounting).
  const initial = await invoke<InitialState>("get_initial_state");
  if (initial.settings !== null) {
    applySettings(initial.settings);
  }

  if (initial.connected) {
    deviceConnected.value = true;
  }
});

onUnmounted(() => {
  for (const unlisten of unlisteners) {
    unlisten();
  }
});
</script>

<template>
  <div class="flex h-full flex-col">
    <!-- Connection status bar -->
    <div
      class="flex items-center gap-2 border-b border-surface-200 px-3 py-1 text-xs dark:border-surface-700"
    >
      <span
        class="size-2 rounded-full"
        :class="deviceConnected ? 'bg-green-500' : 'bg-surface-400'"
      />
      <span class="text-surface-500">
        {{ deviceConnected ? "Device connected" : "No device" }}
      </span>
    </div>

    <!-- Channel strips -->
    <div class="flex flex-1 overflow-hidden">
      <div
        v-for="(settings, i) in channelSettings"
        :key="i"
        class="w-1/4 overflow-hidden"
      >
        <ChannelStrip
          v-if="settings !== null"
          :channel-index="i"
          :settings="settings"
        />
      </div>
    </div>
  </div>
</template>
