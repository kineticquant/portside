import { useState } from "react";
import { api, isLan, type Instance } from "../lib/tauri";

export type InstanceListProps = {
  instances: Instance[];
  onStart?: (id: string) => void | Promise<void>;
  onStop?: (id: string) => void | Promise<void>;
  onRemove?: (id: string) => void | Promise<void>;
  onWipe?: (id: string) => void | Promise<void>;
  onForget?: (id: string) => void | Promise<void>;
  onSelect?: (id: string) => void;
  selectedId?: string | null;
  actionError?: string | null;
  onActionError?: (message: string) => void;
  /** This box's LAN IP for laptop-facing strings. Null = unknown/offline. */
  lanIp?: string | null;
  /** Strict verify-ca mode for connect strings. */
  strict?: boolean;
};

function downloadText(filename: string, text: string) {
  const blob = new Blob([text], { type: "application/x-pem-file" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

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
  onWipe,
  onForget,
  onSelect,
  selectedId,
  actionError,
  onActionError,
  lanIp,
  strict,
}: InstanceListProps) {
  const [copiedId, setCopiedId] = useState<string | null>(null);

  if (instances.length === 0) {
    return (
      <div className="ps-empty">
        <strong>No instances yet</strong>
        Click Create instance, pick an engine and version — your first
        database lands here with a connect string ready to copy.
      </div>
    );
  }

  const handleCopy = async (inst: Instance, lan: boolean) => {
    try {
      await copyText(
        api.connectString(inst, {
          host: lan && lanIp ? lanIp : inst.host || "127.0.0.1",
          strict,
        }),
      );
      setCopiedId(inst.id + (lan ? "-lan" : ""));
      setTimeout(
        () =>
          setCopiedId((cur) =>
            cur === inst.id + (lan ? "-lan" : "") ? null : cur,
          ),
        1500,
      );
    } catch (e) {
      onActionError?.(
        `Copy failed: ${e instanceof Error ? e.message : String(e)}`,
      );
    }
  };

  const handleCert = async (inst: Instance) => {
    try {
      const pem = await api.serverCert(inst.id);
      downloadText(`portside-${inst.id}.crt`, pem);
    } catch (e) {
      onActionError?.(
        `Cert download failed: ${e instanceof Error ? e.message : String(e)}`,
      );
    }
  };

  return (
    <div>
      {actionError ? (
        <p role="alert" className="ps-alert">
          {actionError}
        </p>
      ) : null}
      <table className="ps-table">
        <thead>
          <tr>
            <th>Image</th>
            <th>Endpoint</th>
            <th>Status</th>
            <th>Actions</th>
          </tr>
        </thead>
        <tbody>
          {instances.map((inst) => {
            const lan = isLan(inst);
            const external = inst.origin === "imported";
            return (
            <tr
              key={inst.id}
              onClick={() => onSelect?.(inst.id)}
              className={selectedId === inst.id ? "ps-selected" : undefined}
            >
              <td>
                {inst.engine}:{inst.tag}
                <span className="ps-cell-sub">
                  {external ? (
                    <span className="ps-badge" title="Registered here, running elsewhere">
                      Imported
                    </span>
                  ) : inst.origin === "adopted" ? (
                    <span className="ps-badge" title="Pre-existing container under management">
                      Adopted
                    </span>
                  ) : (
                    inst.container
                  )}
                </span>
              </td>
              <td>
                {inst.host || "127.0.0.1"}:{inst.port}
                <span className="ps-cell-sub">
                  {lan ? (
                    <span
                      className="ps-badge ps-badge-tls"
                      title="Reachable from this network over TLS"
                    >
                      LAN · TLS
                    </span>
                  ) : (
                    "localhost only"
                  )}
                </span>
              </td>
              <td>
                <span
                  className={`ps-dot ${
                    inst.status === "running"
                      ? "ps-dot-running"
                      : "ps-dot-stopped"
                  }`}
                />
                {inst.status}
              </td>
              <td>
                <div className="ps-actions">
                  {!external ? (
                  <div className="ps-act-group" role="group" aria-label="lifecycle">
                    <button
                      type="button"
                      className="ps-btn-ghost"
                      title="Start container"
                      onClick={(e) => {
                        e.stopPropagation();
                        void onStart?.(inst.id);
                      }}
                    >
                      Start
                    </button>
                    <button
                      type="button"
                      className="ps-btn-ghost"
                      title="Inactivate: stop container, keep row + data volume so it can restart"
                      onClick={(e) => {
                        e.stopPropagation();
                        void onStop?.(inst.id);
                      }}
                    >
                      Inactivate
                    </button>
                  </div>
                  ) : null}
                  <div className="ps-act-group" role="group" aria-label="connect">
                    <button
                      type="button"
                      className="ps-btn-ghost"
                      title={api.connectString(inst, { strict })}
                      onClick={(e) => {
                        e.stopPropagation();
                        void handleCopy(inst, false);
                      }}
                    >
                      {copiedId === inst.id ? "Copied!" : "Copy connect"}
                    </button>
                    {lan ? (
                      <>
                        <button
                          type="button"
                          className="ps-btn-ghost"
                          title={
                            lanIp
                              ? api.connectString(inst, { host: lanIp, strict })
                              : "LAN IP unknown (offline?)"
                          }
                          disabled={!lanIp}
                          onClick={(e) => {
                            e.stopPropagation();
                            void handleCopy(inst, true);
                          }}
                        >
                          {copiedId === `${inst.id}-lan` ? "Copied!" : "Copy LAN"}
                        </button>
                        <button
                          type="button"
                          className="ps-btn-ghost"
                          title="Download this instance's server cert (.crt) — your client needs it when Strict TLS is on"
                          onClick={(e) => {
                            e.stopPropagation();
                            void handleCert(inst);
                          }}
                        >
                          Cert
                        </button>
                      </>
                    ) : null}
                  </div>
                  <div className="ps-act-group" role="group" aria-label="danger">
                    {external ? (
                    <button
                      type="button"
                      className="ps-btn-ghost"
                      title="Forget: delete Portside's record only. The server itself is untouched."
                      onClick={(e) => {
                        e.stopPropagation();
                        if (
                          window.confirm(
                            `Forget ${inst.engine} on ${inst.host || "127.0.0.1"}:${inst.port}?\nThis deletes Portside's record only. The server itself is untouched.`,
                          )
                        ) {
                          void onForget?.(inst.id);
                        }
                      }}
                    >
                      Forget
                    </button>
                    ) : (
                    <>
                    <button
                      type="button"
                      className="ps-btn-ghost"
                      title={`Remove: delete container, KEEP data volume ${inst.volume} for extract/restore`}
                      onClick={(e) => {
                        e.stopPropagation();
                        void onRemove?.(inst.id);
                      }}
                    >
                      Remove
                    </button>
                    <button
                      type="button"
                      className="ps-btn-danger"
                      title={
                        inst.volume
                          ? `Wipe: delete container AND data volume ${inst.volume}. Destructive.`
                          : "Wipe unavailable: no known data volume for this container."
                      }
                      disabled={!inst.volume}
                      onClick={(e) => {
                        e.stopPropagation();
                        if (
                          window.confirm(
                            `Wipe ${inst.engine}:${inst.tag} on port ${inst.port}?\nThis deletes container ${inst.container} AND data volume ${inst.volume}. No restore.`,
                          )
                        ) {
                          void onWipe?.(inst.id);
                        }
                      }}
                    >
                      Wipe
                    </button>
                    </>
                    )}
                  </div>
                </div>
              </td>
            </tr>
            );
          })}
        </tbody>
      </table>
      <p className="ps-hint">
        Inactivate = stop, keep data. Remove = drop container, keep volume for
        extract/restore. Wipe = drop container + volume, no restore.
      </p>
    </div>
  );
}
