import { Action, ActionPanel, Form, showToast, Toast, useNavigation } from "@raycast/api";
import { useState } from "react";
import { addModifierRemap, addConditionalRemap, addTapHold, addChord, RemapType } from "./lib/config";
import { restartService } from "./lib/service";
import { ALL_KEYS, MODIFIER_KEYS, MODIFIER_NAMES } from "./lib/keys";

const TYPE_OPTIONS: { value: RemapType; title: string }[] = [
  { value: "modifier_remap", title: "Modifier Remap (hidutil)" },
  { value: "conditional_remap", title: "Conditional Remap" },
  { value: "tap_hold", title: "Tap-Hold" },
  { value: "chord", title: "Chord" },
];

export function AddRemapForm({ onAdd }: { onAdd?: () => void }) {
  const { pop } = useNavigation();
  const [remapType, setRemapType] = useState<RemapType>("conditional_remap");

  async function handleSubmit(values: Record<string, string | string[]>) {
    try {
      switch (remapType) {
        case "modifier_remap":
          addModifierRemap(values.from as string, values.to as string);
          break;
        case "conditional_remap":
          addConditionalRemap(values.modifier as string, values.from as string, values.to as string);
          break;
        case "tap_hold":
          addTapHold(
            values.key as string,
            values.tap as string,
            values.hold as string,
            parseInt(values.timeout_ms as string, 10) || 200,
          );
          break;
        case "chord":
          addChord(
            values.keys as string[],
            values.emit as string,
            parseInt(values.window_ms as string, 10) || 100,
          );
          break;
      }

      restartService();
      onAdd?.();
      await showToast({ style: Toast.Style.Success, title: "Remap added" });
      pop();
    } catch (e) {
      await showToast({ style: Toast.Style.Failure, title: "Failed to add remap", message: String(e) });
    }
  }

  return (
    <Form
      actions={
        <ActionPanel>
          <Action.SubmitForm title="Add Remap" onSubmit={handleSubmit} />
        </ActionPanel>
      }
    >
      <Form.Dropdown id="type" title="Type" value={remapType} onChange={(v) => setRemapType(v as RemapType)}>
        {TYPE_OPTIONS.map((opt) => (
          <Form.Dropdown.Item key={opt.value} value={opt.value} title={opt.title} />
        ))}
      </Form.Dropdown>

      <Form.Separator />

      {remapType === "modifier_remap" && (
        <>
          <Form.Dropdown id="from" title="From Key">
            {MODIFIER_KEYS.map((k) => (
              <Form.Dropdown.Item key={k} value={k} title={k} />
            ))}
          </Form.Dropdown>
          <Form.Dropdown id="to" title="To Key">
            {MODIFIER_KEYS.map((k) => (
              <Form.Dropdown.Item key={k} value={k} title={k} />
            ))}
          </Form.Dropdown>
        </>
      )}

      {remapType === "conditional_remap" && (
        <>
          <Form.Dropdown id="modifier" title="Modifier">
            {MODIFIER_NAMES.map((m) => (
              <Form.Dropdown.Item key={m} value={m} title={m} />
            ))}
          </Form.Dropdown>
          <Form.Dropdown id="from" title="From Key">
            {ALL_KEYS.map((k) => (
              <Form.Dropdown.Item key={k} value={k} title={k} />
            ))}
          </Form.Dropdown>
          <Form.Dropdown id="to" title="To Key">
            {ALL_KEYS.map((k) => (
              <Form.Dropdown.Item key={k} value={k} title={k} />
            ))}
          </Form.Dropdown>
        </>
      )}

      {remapType === "tap_hold" && (
        <>
          <Form.Dropdown id="key" title="Key">
            {ALL_KEYS.map((k) => (
              <Form.Dropdown.Item key={k} value={k} title={k} />
            ))}
          </Form.Dropdown>
          <Form.Dropdown id="tap" title="Tap Action">
            {ALL_KEYS.map((k) => (
              <Form.Dropdown.Item key={k} value={k} title={k} />
            ))}
          </Form.Dropdown>
          <Form.Dropdown id="hold" title="Hold Action">
            {ALL_KEYS.map((k) => (
              <Form.Dropdown.Item key={k} value={k} title={k} />
            ))}
          </Form.Dropdown>
          <Form.TextField id="timeout_ms" title="Timeout (ms)" defaultValue="200" />
        </>
      )}

      {remapType === "chord" && (
        <>
          <Form.TagPicker id="keys" title="Keys">
            {ALL_KEYS.map((k) => (
              <Form.TagPicker.Item key={k} value={k} title={k} />
            ))}
          </Form.TagPicker>
          <Form.Dropdown id="emit" title="Emit Key">
            {ALL_KEYS.map((k) => (
              <Form.Dropdown.Item key={k} value={k} title={k} />
            ))}
          </Form.Dropdown>
          <Form.TextField id="window_ms" title="Window (ms)" defaultValue="100" />
        </>
      )}
    </Form>
  );
}

// Default export for the standalone command
export default function Command() {
  return <AddRemapForm />;
}
