import { useState } from "react";
import { api, type Instance } from "../lib/tauri";

export type InstanceListProps = {
  instances: Instance[];
  onStart?: (id: string) => void | Promise<void>;
  onStop?: (id: string) => void | Promise<void>;
  onRemove?: (id: string) => void | Promise<void>;
  onSelect?: (id: string) => void;
  selectedId?: string | null;
  actionError?: string | null;
};

async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const area = document.createElement("textarea");
    area.value = text;
    document.body.appendChild(area);
    area.select();
    document.execCommand("copy");
    document.body.removeChild(area);
  }
}

export default function InstanceList({
  instances,
  onStart,
  onStop,
  onRemove,
  onSelect,
  selectedId,
  actionError,
}: InstanceListProps) {
  const [copiedId, setCopiedId] = useState<string | null>(null);

  if (instances.length === 0) {
    return <p>No instances yet. Create one to get started.</p>;
  }

  const handleCopy = async (inst: Instance) => {
    await copyText(api.connectString(inst));
    setCopiedId(inst.id);
    setTimeout(() => setCopiedId((cur) => (cur === inst.id ? null : cur)), 1500);
  };

  return (
    <div>
      {actionError ? <p role="alert">{actionError}</p> : null}
      <table>
        <thead>
          <tr>
            <th>Engine</th>
            <th>Tag</th>
            <th>Port</th>
            <th>Status</th>
            <th>Actions</th>
          </tr>
        </thead>
        <tbody>
          {instances.map((inst) => (
            <tr
              key={inst.id}
              onClick={() => onSelect?.(inst.id)}
              style={
                selectedId === inst.id ? { fontWeight: "bold" } : undefined
              }
            >
              <td>{inst.engine}</td>
              <td>{inst.tag}</td>
              <td>{inst.port}</td>
              <td>{inst.status}</td>
              <td>
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    void onStart?.(inst.id);
                  }}
                >
                  Start
                </button>
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    void onStop?.(inst.id);
                  }}
                >
                  Stop
                </button>
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    void onRemove?.(inst.id);
                  }}
                >
                  Delete
                </button>
                <button
                  type="button"
                  title={api.connectString(inst)}
                  onClick={(e) => {
                    e.stopPropagation();
                    void handleCopy(inst);
                  }}
                >
                  {copiedId === inst.id ? "Copied!" : "Copy connect"}
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
