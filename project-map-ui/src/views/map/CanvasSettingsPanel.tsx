import { X } from "lucide-react";
import type { CanvasSettings } from "../../state/session";

interface CanvasSettingsPanelProps {
  open: boolean;
  settings: CanvasSettings;
  onChange: (settings: CanvasSettings) => void;
  onClose: () => void;
}

export function CanvasSettingsPanel({
  open,
  settings,
  onChange,
  onClose,
}: CanvasSettingsPanelProps) {
  if (!open) {
    return null;
  }
  return (
    <aside
      id="settingsPanel"
      className="settings-panel"
      aria-label="Canvas settings"
    >
      <header>
        <h2>Canvas settings</h2>
        <button
          id="closeSettings"
          type="button"
          className="icon-button"
          aria-label="Close canvas settings"
          onClick={onClose}
        >
          <X size={16} />
        </button>
      </header>
      <div className="settings-fields">
        <label>
          <span>Wheel behavior</span>
          <select
            id="wheelMode"
            value={settings.wheelMode}
            onChange={(event) =>
              onChange({
                ...settings,
                wheelMode: event.currentTarget.value as "pan" | "zoom",
              })
            }
          >
            <option value="pan">Pan</option>
            <option value="zoom">Zoom</option>
          </select>
        </label>
        <label>
          <span>
            Pan sensitivity{" "}
            <output id="panSensitivityValue">
              {Math.round(settings.panSensitivity * 100)}%
            </output>
          </span>
          <input
            id="panSensitivity"
            type="range"
            min="0.2"
            max="1.5"
            step="0.05"
            value={settings.panSensitivity}
            onChange={(event) =>
              onChange({
                ...settings,
                panSensitivity: Number(event.currentTarget.value),
              })
            }
          />
        </label>
        <label>
          <span>
            Zoom sensitivity{" "}
            <output id="zoomSensitivityValue">
              {Math.round(settings.zoomSensitivity * 100)}%
            </output>
          </span>
          <input
            id="zoomSensitivity"
            type="range"
            min="0.1"
            max="1.25"
            step="0.05"
            value={settings.zoomSensitivity}
            onChange={(event) =>
              onChange({
                ...settings,
                zoomSensitivity: Number(event.currentTarget.value),
              })
            }
          />
        </label>
      </div>
    </aside>
  );
}
