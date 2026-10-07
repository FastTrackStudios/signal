// The inherit icon: an arrow coming down from above — what a preset higher
// up chose (a patch's Core, the Core's Amp), not set here.

export function InheritIcon({ colour = "currentColor", size = 12 }: { colour?: string; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 12 12" aria-hidden style={{ flexShrink: 0, display: "block" }}>
      <path d="M3 1.5v4.2a2 2 0 0 0 2 2h5" fill="none" stroke={colour} strokeWidth="1.4" strokeLinecap="round" />
      <path d="M7.8 5.2 10.3 7.7 7.8 10.2" fill="none" stroke={colour} strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}
