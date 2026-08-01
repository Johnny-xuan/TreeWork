import { Minus, Plus, X } from "lucide-react";
import type { ReaderTheme, ReadingMeasure } from "../types";

interface ReadingSettingsProps {
  open: boolean;
  textScale: number;
  measure: ReadingMeasure;
  theme: ReaderTheme;
  onClose: () => void;
  onTextScaleChange: (value: number) => void;
  onMeasureChange: (value: ReadingMeasure) => void;
  onThemeChange: (value: ReaderTheme) => void;
}

export function ReadingSettings({
  open,
  textScale,
  measure,
  theme,
  onClose,
  onTextScaleChange,
  onMeasureChange,
  onThemeChange,
}: ReadingSettingsProps) {
  if (!open) {
    return null;
  }

  return (
    <section className="settings-panel" aria-label="Reading settings">
      <header>
        <h2>Reading settings</h2>
        <button
          type="button"
          className="icon-button"
          aria-label="Close reading settings"
          title="Close"
          onClick={onClose}
        >
          <X size={16} />
        </button>
      </header>

      <div className="setting-row">
        <span>Text size</span>
        <div className="stepper" aria-label="Text size">
          <button
            type="button"
            aria-label="Decrease text size"
            disabled={textScale <= -1}
            onClick={() => onTextScaleChange(Math.max(-1, textScale - 1))}
          >
            <Minus size={15} />
          </button>
          <output>{textScale === -1 ? "S" : textScale === 0 ? "M" : "L"}</output>
          <button
            type="button"
            aria-label="Increase text size"
            disabled={textScale >= 1}
            onClick={() => onTextScaleChange(Math.min(1, textScale + 1))}
          >
            <Plus size={15} />
          </button>
        </div>
      </div>

      <fieldset>
        <legend>Measure</legend>
        <div className="segmented-control">
          {(["compact", "standard", "wide"] as const).map((option) => (
            <button
              type="button"
              key={option}
              className={measure === option ? "is-active" : ""}
              aria-pressed={measure === option}
              onClick={() => onMeasureChange(option)}
            >
              {option === "compact" ? "Narrow" : option === "standard" ? "Standard" : "Wide"}
            </button>
          ))}
        </div>
      </fieldset>

      <fieldset>
        <legend>Theme</legend>
        <div className="segmented-control">
          {(["light", "dark", "system"] as const).map((option) => (
            <button
              type="button"
              key={option}
              className={theme === option ? "is-active" : ""}
              aria-pressed={theme === option}
              onClick={() => onThemeChange(option)}
            >
              {option[0].toUpperCase() + option.slice(1)}
            </button>
          ))}
        </div>
      </fieldset>
    </section>
  );
}
