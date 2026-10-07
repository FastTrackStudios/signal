// An iPhone 16 Pro beside the iPad, at the same real scale: 402 × 874
// points, 1206 × 2622 px at 460 ppi (2.62 × 5.70 in of glass) in a
// 71.5 × 149.6 mm body (Apple's tech specs). The screen's corners are
// ~62 pt; the body's are concentric with them. The Dynamic Island is a
// 126 × 37 pt pill 11 pt from the top edge (in landscape, the right edge —
// the phone turned left, as it is held on a stand); the safe areas are 62 pt at the
// island's edge, 34 pt (portrait) or 21 pt (landscape) at the home
// indicator. Both phones read the same store as the iPad, as linked
// remotes do.

import { useState, type ReactNode } from "react";
import { StageCtx } from "./ui/stage";

const PT = { w: 402, h: 874 };
const GLASS_IN = 1206 / 460;
const BODY_MM = { w: 71.5, h: 149.6 };
const CORNER = 62;
const ISLAND = { w: 126, h: 37, inset: 11 };

/** The phone at `pagePpi` page pixels per real inch. */
export function Phone({ landscape, pagePpi, children }: { landscape?: boolean; pagePpi: number; children: ReactNode }) {
  const scale = (GLASS_IN * pagePpi) / PT.w;
  const mmPerPt = (GLASS_IN * 25.4) / PT.w;
  const bezel = (BODY_MM.w / mmPerPt - PT.w) / 2;
  const sw = landscape ? PT.h : PT.w;
  const sh = landscape ? PT.w : PT.h;
  const ow = sw + bezel * 2;
  const oh = sh + bezel * 2;
  const [el, setEl] = useState<HTMLDivElement | null>(null);
  // Safe area: the island's edge and the home indicator's.
  const safe = landscape ? { top: 0, right: 62, bottom: 21, left: 62 } : { top: 62, right: 0, bottom: 34, left: 0 };
  return (
    <div style={{ width: ow * scale, height: oh * scale, flexShrink: 0 }}>
      <div style={{ width: ow, height: oh, transform: `scale(${scale})`, transformOrigin: "0 0" }}>
        <div
          style={{
            position: "relative",
            width: ow,
            height: oh,
            padding: bezel,
            borderRadius: CORNER + bezel,
            background: "#050506",
            boxShadow: "inset 0 0 0 1.5px #a1a1aa, inset 0 0 0 3px #2a2a2e, 0 24px 60px rgba(0,0,0,0.5)",
          }}
        >
          <StageCtx.Provider value={{ el, scale }}>
            <div ref={setEl} style={{ position: "relative", width: sw, height: sh, borderRadius: CORNER, overflow: "hidden", background: "var(--desk)" }}>
              <div style={{ position: "absolute", top: safe.top, right: safe.right, bottom: safe.bottom, left: safe.left, display: "flex", flexDirection: "column", minHeight: 0 }}>{children}</div>
              {!landscape && <StatusBar />}
              {/* The Dynamic Island. */}
              <span
                aria-hidden
                style={{
                  position: "absolute",
                  zIndex: 100,
                  background: "#000",
                  borderRadius: 999,
                  ...(landscape
                    ? { right: ISLAND.inset, top: (sh - ISLAND.w) / 2, width: ISLAND.h, height: ISLAND.w }
                    : { top: ISLAND.inset, left: (sw - ISLAND.w) / 2, width: ISLAND.w, height: ISLAND.h }),
                }}
              />
              {/* The home indicator. */}
              <span aria-hidden style={{ position: "absolute", zIndex: 100, left: "50%", bottom: 8, width: landscape ? 200 : 134, height: 5, marginLeft: landscape ? -100 : -67, borderRadius: 3, background: "rgba(255,255,255,0.7)" }} />
            </div>
          </StageCtx.Provider>
        </div>
      </div>
    </div>
  );
}

/** The portrait status bar, either side of the island. */
function StatusBar() {
  return (
    <div style={{ position: "absolute", top: 0, left: 0, right: 0, height: 54, display: "flex", alignItems: "center", justifyContent: "space-between", padding: "4px 34px 0 46px", fontSize: 16, fontWeight: 650, color: "var(--ink)", pointerEvents: "none" }}>
      <span className="num">9:41</span>
      <span style={{ display: "flex", alignItems: "center", gap: 6 }}>
        <svg width="18" height="12" viewBox="0 0 18 12" aria-hidden>
          {[0, 1, 2, 3].map((i) => (
            <rect key={i} x={i * 4.6} y={9 - i * 2.8} width="3.2" height={3 + i * 2.8} rx="0.8" fill="currentColor" />
          ))}
        </svg>
        <svg width="26" height="13" viewBox="0 0 26 13" aria-hidden>
          <rect x="0.5" y="0.5" width="22" height="12" rx="3.5" fill="none" stroke="currentColor" opacity="0.4" />
          <rect x="2.5" y="2.5" width="16" height="8" rx="2" fill="currentColor" />
          <path d="M24.5 4.5v4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" opacity="0.4" />
        </svg>
      </span>
    </div>
  );
}
