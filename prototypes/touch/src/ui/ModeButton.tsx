// The footswitch mode, as one button: it names the mode the switches are
// in (green-edged — live rig state, shared by every remote) and opens a
// menu of the three, each saying what the switches will do. One button on
// every device; never a blind rotate — a wrong mode mid-song changes what
// all ten switches under your feet do.

import { setPerformMode, useStore } from "../store";
import { MODES } from "../App";
import { Menu, useMenu, type MenuItem, type Picked } from "./Menu";

export function ModeButton({ height = 36, wide }: { height?: number; wide?: boolean }) {
  const s = useStore();
  const menu = useMenu();
  const mode = MODES.find((m) => m.id === s.performMode)!;
  const items: MenuItem[] = [
    { kind: "head", label: "Footswitches play" },
    ...MODES.map((m) => ({ kind: "run" as const, id: m.id, label: m.label, detail: m.id === "preset" ? "presets" : m.id === "profile" ? "its stacks" : "the set", checked: m.id === s.performMode })),
  ];
  const onPick = (p: Picked) => setPerformMode(p.id as typeof s.performMode);
  return (
    <>
      <button
        onClick={(e) => {
          const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
          menu.setOpen({ at: { x: r.left, y: r.bottom + 4 } });
        }}
        aria-haspopup="menu"
        aria-label={`Footswitch mode: ${mode.label} — change`}
        title={mode.hint}
        style={{
          alignSelf: "center",
          height,
          minWidth: wide ? 132 : undefined,
          padding: "0 10px 0 12px",
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 10,
          borderRadius: "var(--r)",
          fontSize: 14,
          fontWeight: 700,
          color: "var(--ink)",
          background: "var(--pressed-bg)",
          boxShadow: "var(--pressed-shadow), inset 0 -2px 0 var(--live)",
        }}
      >
        <span style={{ display: "flex", alignItems: "baseline", gap: 7 }}>
          {wide && (
            <span className="t-label" style={{ fontSize: 9.5, color: "var(--ink-3)" }}>
              Mode
            </span>
          )}
          {mode.label}
        </span>
        <svg width="10" height="6" viewBox="0 0 10 6" aria-hidden style={{ color: "var(--ink-3)", transform: menu.open ? "rotate(180deg)" : undefined }}>
          <path d="M1 1 L5 5 L9 1" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </button>
      {menu.open && <Menu at={menu.open.at} items={items} onPick={onPick} onClose={menu.close} />}
    </>
  );
}
