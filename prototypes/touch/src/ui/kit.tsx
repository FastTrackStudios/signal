// The shared pieces: the side sheet every picker opens in, the bar's tabs,
// and plain buttons — standard controls the hand already knows, set in the
// world's type and taped where they mean "this one".

import type { CSSProperties, ReactNode } from "react";

/** A sheet that slides over the right side: a picker, never a modal. */
export function SideSheet({
  title,
  sub,
  onClose,
  children,
  width = 460,
}: {
  title: string;
  sub?: string;
  onClose: () => void;
  children: ReactNode;
  width?: number;
}) {
  return (
    <div style={{ position: "absolute", inset: 0, zIndex: 40, display: "flex", justifyContent: "flex-end" }}>
      <button
        aria-label="Close"
        onClick={onClose}
        style={{ position: "absolute", inset: 0, background: "rgba(18,18,18,0.22)", cursor: "default" }}
      />
      <section
        className="sheet"
        style={{
          position: "relative",
          width,
          maxWidth: "92%",
          height: "100%",
          borderRadius: 0,
          display: "flex",
          flexDirection: "column",
          animation: "sheet-in 220ms var(--ease) both",
        }}
      >
        <header style={{ display: "flex", alignItems: "flex-start", gap: 12, padding: "18px 18px 14px 22px", borderBottom: "1px solid var(--rule)" }}>
          <div style={{ flex: 1, minWidth: 0 }}>
            <h2 className="t-marker" style={{ margin: 0, fontSize: 26 }}>
              {title}
            </h2>
            {sub && (
              <div className="t-meta" style={{ marginTop: 4 }}>
                {sub}
              </div>
            )}
          </div>
          <Button onClick={onClose}>Done</Button>
        </header>
        <div style={{ flex: 1, minHeight: 0, overflowY: "auto" }}>{children}</div>
      </section>
      <style>{`@keyframes sheet-in { from { transform: translateX(24px); opacity: 0 } to { transform: none; opacity: 1 } }`}</style>
    </div>
  );
}

/** A button: ink outline, or solid ink for the one thing you came to do. */
export function Button({
  children,
  onClick,
  primary,
  disabled,
  style,
  title,
}: {
  children: ReactNode;
  onClick?: () => void;
  primary?: boolean;
  disabled?: boolean;
  style?: CSSProperties;
  title?: string;
}) {
  return (
    <button
      className="pressable"
      onClick={onClick}
      disabled={disabled}
      title={title}
      style={{
        minHeight: "var(--hit)",
        padding: "0 18px",
        borderRadius: "var(--r)",
        border: `2px solid ${disabled ? "var(--rule-strong)" : "var(--ink)"}`,
        background: primary ? "var(--ink)" : "var(--sheet)",
        color: disabled ? "var(--ink-3)" : primary ? "#fff" : "var(--ink)",
        fontWeight: 760,
        fontSize: 15,
        whiteSpace: "nowrap",
        cursor: disabled ? "default" : "pointer",
        ...style,
      }}
    >
      {children}
    </button>
  );
}

/** Tabs: the one in use is taped down in black. */
export function Tabs<T extends string>({
  options,
  value,
  onChange,
  size = "md",
  wrap = false,
}: {
  options: readonly T[] | { id: T; label: string; count?: number }[];
  value: T;
  onChange: (v: T) => void;
  size?: "md" | "lg";
  /** Wrap onto a second row instead of scrolling (a narrow column). */
  wrap?: boolean;
}) {
  const opts = (options as readonly (T | { id: T; label: string; count?: number })[]).map((o) =>
    typeof o === "string" ? { id: o as T, label: o as string, count: undefined } : o,
  );
  return (
    <div role="tablist" style={{ display: "flex", gap: 4, flexWrap: wrap ? "wrap" : "nowrap", overflowX: wrap ? "visible" : "auto" }}>
      {opts.map((o) => {
        const on = o.id === value;
        return (
          <button
            key={o.id}
            role="tab"
            aria-selected={on}
            onClick={() => onChange(o.id)}
            className={on ? "" : "pressable"}
            style={{
              minHeight: size === "lg" ? 48 : 40,
              padding: size === "lg" ? "0 18px" : "0 13px",
              borderRadius: 2,
              background: on ? "var(--tape-gaffer)" : "transparent",
              color: on ? "#fff" : "var(--ink-2)",
              fontWeight: on ? 800 : 650,
              fontStretch: "96%",
              fontSize: size === "lg" ? 17 : 15,
              whiteSpace: "nowrap",
              display: "inline-flex",
              alignItems: "center",
              gap: 7,
            }}
          >
            {o.label}
            {o.count !== undefined && (
              <span className="num" style={{ fontSize: 12, opacity: 0.7, fontWeight: 600 }}>
                {o.count}
              </span>
            )}
          </button>
        );
      })}
    </div>
  );
}

/** A song's key in a ruled box. */
export function KeyBox({ k, big }: { k: string; big?: boolean }) {
  if (!k) return null;
  return (
    <span
      className="num"
      style={{
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        minWidth: big ? 44 : 30,
        height: big ? 44 : 28,
        padding: "0 6px",
        border: "2px solid var(--ink)",
        borderRadius: 2,
        fontWeight: 800,
        fontSize: big ? 22 : 15,
        flexShrink: 0,
      }}
    >
      {k}
    </span>
  );
}
