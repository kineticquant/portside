export type InfoTipProps = {
  /** Accessible name, e.g. "About engine". */
  label: string;
  /** Short explainer shown on hover/focus. */
  text: string;
  /** Tooltip alignment. Use "right" for triggers near the viewport edge (e.g. header). */
  align?: "center" | "right";
  /** Tooltip direction. Use "down" for triggers near the top of the viewport (e.g. header). */
  direction?: "up" | "down";
};

/** Small ⓘ button with a CSS-only tooltip. No deps, keyboard accessible. */
export default function InfoTip({
  label,
  text,
  align = "center",
  direction = "up",
}: InfoTipProps) {
  const cls =
    "ps-tip" +
    (align === "right" ? " ps-tip-right" : "") +
    (direction === "down" ? " ps-tip-down" : "");
  return (
    <span className="ps-info">
      <button type="button" className="ps-info-btn" aria-label={label}>
        i
      </button>
      <span className={cls} role="tooltip">
        {text}
      </span>
    </span>
  );
}
