// The override icon: a square laid over another — something placed over
// what the preset has. It marks anything that is currently an override:
// browser rows and kinds, a preset's column, a section or song in the
// setlist. Drawn in the effect's colour where there is one.

export function OverrideIcon({ colour = "currentColor", size = 12, title }: { colour?: string; size?: number; title?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 12 12" role={title ? "img" : undefined} aria-label={title} aria-hidden={title ? undefined : true} style={{ flexShrink: 0, display: "block" }}>
      {title && <title>{title}</title>}
      {/* What's under: the preset's own, dimmed. */}
      <rect x="1" y="1" width="7" height="7" rx="1.6" fill="none" stroke={colour} strokeWidth="1.3" opacity="0.45" />
      {/* What's over it: the override, solid. */}
      <rect x="4" y="4" width="7" height="7" rx="1.6" fill={colour} />
    </svg>
  );
}
