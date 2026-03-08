<script setup lang="ts">
import { invoke } from "@tauri-apps/api/core";
import InputNumber from "primevue/inputnumber";
import InputText from "primevue/inputtext";
import SelectButton from "primevue/selectbutton";
import ToggleButton from "primevue/togglebutton";
import { ref, watch } from "vue";

import type {
  ExpressionChannelSettingsPatch,
  InputMode,
  SettingsPatch,
} from "../types/settings";

const props = defineProps<{ channelIndex: number }>();

// --- State ---

const inputMode = ref<InputMode>("Compat");

// Continuous
const minimumInput = ref(0.0);
const maximumInput = ref(1.0);
const minimumOutput = ref<number>(0);
const maximumOutput = ref<number>(127);
const drive = ref(0.5);

// Switch
const invertPolarity = ref(false);
const releasedValue = ref<number>(0);
const pressedValue = ref<number>(127);

// Common
const cc = ref<number>(props.channelIndex);
const label = ref("");

// --- Helpers ---

const modeOptions = [
  { label: "C", value: "Continuous" },
  { label: "S", value: "Switch" },
  { label: "X", value: "Compat" },
];

function labelToBytes(str: string): number[] {
  const bytes = new Array(32).fill(0);
  for (let i = 0; i < Math.min(str.length, 32); i++) {
    bytes[i] = str.charCodeAt(i);
  }

  return bytes;
}

async function sendPatch(patch: ExpressionChannelSettingsPatch): Promise<void> {
  const settingsPatch: SettingsPatch = {
    ExpressionChannel: [props.channelIndex, patch],
  };
  await invoke("patch_settings", { patch: settingsPatch });
}

// --- Watchers ---

watch(inputMode, v => sendPatch({ InputMode: v }));
watch(cc, v => v != null && sendPatch({ CC: v }));
watch(label, v => sendPatch({ Label: labelToBytes(v) }));

watch(minimumInput, v => v != null && sendPatch({ ContinuousMinimumInput: v }));
watch(maximumInput, v => v != null && sendPatch({ ContinuousMaximumInput: v }));
watch(minimumOutput, v => v != null && sendPatch({ ContinuousMinimumOutput: v }));
watch(maximumOutput, v => v != null && sendPatch({ ContinuousMaximumOutput: v }));
watch(drive, v => v != null && sendPatch({ ContinuousDrive: v }));

watch(invertPolarity, v => sendPatch({ SwitchInvertPolarity: v }));
watch(releasedValue, v => v != null && sendPatch({ SwitchReleasedValue: v }));
watch(pressedValue, v => v != null && sendPatch({ SwitchPressedValue: v }));
</script>

<template>
  <div class="flex h-full min-w-0 flex-col border-r border-surface-200 p-2 dark:border-surface-700">
    <!-- Channel index -->
    <div class="mb-2 text-center text-6xl font-bold text-surface-300 dark:text-surface-600">
      {{ channelIndex }}
    </div>

    <!-- Mode selector -->
    <SelectButton
      v-model="inputMode"
      :options="modeOptions"
      option-label="label"
      option-value="value"
      class="w-full"
      fluid
    />

    <!-- Mode-specific settings -->
    <div class="mt-3 flex flex-1 flex-col gap-2 text-sm">
      <template v-if="inputMode === 'Continuous'">
        <label class="flex flex-col gap-1">
          <span class="text-surface-500">Drive</span>
          <InputNumber
            v-model="drive"
            :min="0"
            :max="1"
            :step="0.01"
            :min-fraction-digits="2"
            :max-fraction-digits="2"
            fluid
          />
        </label>
        <label class="flex flex-col gap-1">
          <span class="text-surface-500">In Min</span>
          <InputNumber
            v-model="minimumInput"
            :min="0"
            :max="1"
            :step="0.01"
            :min-fraction-digits="2"
            :max-fraction-digits="2"
            fluid
          />
        </label>
        <label class="flex flex-col gap-1">
          <span class="text-surface-500">In Max</span>
          <InputNumber
            v-model="maximumInput"
            :min="0"
            :max="1"
            :step="0.01"
            :min-fraction-digits="2"
            :max-fraction-digits="2"
            fluid
          />
        </label>
        <label class="flex flex-col gap-1">
          <span class="text-surface-500">Out Min</span>
          <InputNumber
            v-model="minimumOutput"
            :min="0"
            :max="127"
            :step="1"
            fluid
          />
        </label>
        <label class="flex flex-col gap-1">
          <span class="text-surface-500">Out Max</span>
          <InputNumber
            v-model="maximumOutput"
            :min="0"
            :max="127"
            :step="1"
            fluid
          />
        </label>
      </template>

      <template v-else-if="inputMode === 'Switch'">
        <label class="flex flex-col gap-1">
          <span class="text-surface-500">Invert Polarity</span>
          <ToggleButton
            v-model="invertPolarity"
            on-label="On"
            off-label="Off"
            fluid
          />
        </label>
        <label class="flex flex-col gap-1">
          <span class="text-surface-500">Released Value</span>
          <InputNumber
            v-model="releasedValue"
            :min="0"
            :max="127"
            :step="1"
            fluid
          />
        </label>
        <label class="flex flex-col gap-1">
          <span class="text-surface-500">Pressed Value</span>
          <InputNumber
            v-model="pressedValue"
            :min="0"
            :max="127"
            :step="1"
            fluid
          />
        </label>
      </template>

      <template v-else>
        <p class="text-center text-xs text-surface-400">
          No additional settings
        </p>
      </template>
    </div>

    <!-- CC + Label (bottom) -->
    <div class="mt-3 flex flex-col gap-2 border-t border-surface-200 pt-3 text-sm dark:border-surface-700">
      <label class="flex flex-col gap-1">
        <span class="text-surface-500">CC</span>
        <InputNumber
          v-model="cc"
          :min="0"
          :max="127"
          :step="1"
          fluid
        />
      </label>
      <label class="flex flex-col gap-1">
        <span class="text-surface-500">Label</span>
        <InputText
          v-model="label"
          :maxlength="32"
          fluid
        />
      </label>
    </div>
  </div>
</template>
