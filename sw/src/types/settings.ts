// ---------------------------------------------------------------------------
// Read types — mirror the Rust Settings structs (serde default naming)
// ---------------------------------------------------------------------------

export type InputMode = "Continuous" | "Switch" | "Compat";

export interface ContinuousSettings {
  minimum_input: number;
  maximum_input: number;
  minimum_output: number;
  maximum_output: number;
  drive: number;
}

export interface SwitchSettings {
  invert_polarity: boolean;
  released_value: number;
  pressed_value: number;
}

export interface InputSettings {
  mode: InputMode;
  continuous: ContinuousSettings;
  switch: SwitchSettings;
}

export interface ExpressionChannelSettings {
  input: InputSettings;
  cc: number;
  /** 32-byte null-padded UTF-8 label, serialised as number[] */
  label: number[];
}

export interface ExpressionGroupSettings {
  channels: [
    ExpressionChannelSettings,
    ExpressionChannelSettings,
    ExpressionChannelSettings,
    ExpressionChannelSettings,
  ];
}

export interface Settings {
  expression: ExpressionGroupSettings;
}

// ---------------------------------------------------------------------------
// Patch types — mirror the Rust SettingsPatch enums
// ---------------------------------------------------------------------------

export type ExpressionChannelSettingsPatch =
  | { Label: number[] } |
  { CC: number } |
  { InputMode: InputMode } |
  { ContinuousMinimumInput: number } |
  { ContinuousMaximumInput: number } |
  { ContinuousMinimumOutput: number } |
  { ContinuousMaximumOutput: number } |
  { ContinuousDrive: number } |
  { SwitchInvertPolarity: boolean } |
  { SwitchReleasedValue: number } |
  { SwitchPressedValue: number };

// Tuple enum variant — serde serialises as { "ExpressionChannel": [index, patch] }
export type SettingsPatch = {
  ExpressionChannel: [number, ExpressionChannelSettingsPatch];
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

export function labelBytesToString(bytes: number[]): string {
  const end = bytes.indexOf(0);
  const slice = end === -1 ? bytes : bytes.slice(0, end);
  return String.fromCharCode(...slice);
}

export function stringToLabelBytes(str: string): number[] {
  const bytes = new Array<number>(32).fill(0);
  for (let i = 0; i < Math.min(str.length, 32); i++) {
    bytes[i] = str.charCodeAt(i);
  }
  return bytes;
}
