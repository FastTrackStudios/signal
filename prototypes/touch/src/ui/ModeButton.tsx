// The footswitch mode, as one button: it names the mode the switches are
// in (a green line along the bar's foot — live rig state, shared by every
// remote) and opens a
// menu of the three, each saying what the switches will do. One button on
// every device; never a blind rotate — a wrong mode mid-song changes what
// all ten switches under your feet do.

import { setPerformMode, useStore } from "../store";
import { MODES } from "../App";
import { Menu, useMenu, type MenuItem, type Picked } from "./Menu";

export function ModeButton({ wide }: { wide?: boolean }) {
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
      {/* Part of the bar, not a card on it: full height, flat, the mode's
          name and a chevron, the live green as a line along the bar's foot. */}
      <button
        className="pressable"
        onClick={(e) => {
          const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
          menu.setOpen({ at: { x: r.left, y: r.bottom } });
        }}
        aria-haspopup="menu"
        aria-label={`Footswitch mode: ${mode.label} — change`}
        title={mode.hint}
        style={{
          position: "relative",
          alignSelf: "stretch",
          minWidth: wide ? 120 : undefined,
          padding: wide ? "0 14px 0 16px" : "0 10px 0 12px",
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 10,
          fontSize: wide ? 15 : 14,
          fontWeight: 700,
          color: "var(--ink)",
          background: menu.open ? "rgba(255,255,255,0.05)" : "transparent",
        }}
      >
        {mode.label}
        <svg width="10" height="6" viewBox="0 0 10 6" aria-hidden style={{ color: "var(--ink-3)", transform: menu.open ? "rotate(180deg)" : undefined, transition: "transform 160ms var(--ease)" }}>
          <path d="M1 1 L5 5 L9 1" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
        <span aria-hidden style={{ position: "absolute", left: 8, right: 8, bottom: 0, height: 2, borderRadius: 1, background: "var(--live)" }} />
      </button>
      {menu.open && <Menu at={menu.open.at} items={items} onPick={onPick} onClose={menu.close} />}
    </>
  );
}
