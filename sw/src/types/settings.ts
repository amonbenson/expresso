export type InputMode = "Continuous" | "Switch" | "Compat";

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
