// Each profile's icon: a sound-world you can tell apart at a glance, drawn
// on one 16-unit grid with one stroke weight. A profile with no icon of its
// own shows the stack of layers every profile is.

import type { ReactNode } from "react";

const S = { fill: "none", stroke: "currentColor", strokeWidth: 1.5, strokeLinecap: "round", strokeLinejoin: "round" } as const;

const ICONS: Record<string, ReactNode> = {
  // A flame.
  Worship: <path {...S} d="M8 14.5c-2.8 0-4.5-1.9-4.5-4.3 0-2.6 2.2-3.9 2.6-6.7 1.5 1 2.2 2.4 2.2 3.6.7-.5 1.1-1.3 1.2-2.2 1.6 1.3 3 3.1 3 5.3 0 2.4-1.7 4.3-4.5 4.3Z" />,
  // A crescent moon: late, slow, low.
  Blues: <path {...S} d="M12.8 10.4A5.6 5.6 0 0 1 5.6 3.2 5.6 5.6 0 1 0 12.8 10.4Z" />,
  // A bolt.
  Metal: <path {...S} d="M9.2 1.5 3.5 9h4.1l-.9 5.5L12.5 7H8.4l.8-5.5Z" />,
  // A speaker cabinet.
  Rock: (
    <>
      <rect {...S} x="2.5" y="2" width="11" height="12" rx="1.5" />
      <circle {...S} cx="8" cy="9" r="3" />
      <path {...S} d="M5 4.5h1" />
    </>
  ),
  // A four-point sparkle.
  Funk: <path {...S} d="M8 1.5c.5 3.4 1.9 4.9 5.5 5.5-3.6.6-5 2.1-5.5 6.5-.5-4.4-1.9-5.9-5.5-6.5C6.1 6.4 7.5 4.9 8 1.5Z" />,
  // Two beamed notes.
  Jazz: (
    <>
      <path {...S} d="M5.5 11.5v-8l7-1.5v8" />
      <ellipse cx="4" cy="11.8" rx="1.9" ry="1.5" fill="currentColor" />
      <ellipse cx="11" cy="10.3" rx="1.9" ry="1.5" fill="currentColor" />
    </>
  ),
  // A cassette.
  MkGee: (
    <>
      <rect {...S} x="1.5" y="3.5" width="13" height="9" rx="1.5" />
      <circle {...S} cx="5.5" cy="7.5" r="1.3" />
      <circle {...S} cx="10.5" cy="7.5" r="1.3" />
      <path {...S} d="M4.5 12.5 5.5 10.5h5l1 2" />
    </>
  ),
  // A record.
  Indie: (
    <>
      <circle {...S} cx="8" cy="8" r="6.2" />
      <circle {...S} cx="8" cy="8" r="2" />
      <path {...S} d="M8 3.6a4.4 4.4 0 0 1 4.4 4.4" opacity="0.6" />
    </>
  ),
  // A flask.
  Experimental: (
    <>
      <path {...S} d="M6 1.8h4M6.7 1.8v4.4L2.9 12.6a1.2 1.2 0 0 0 1 1.9h8.2a1.2 1.2 0 0 0 1-1.9L9.3 6.2V1.8" />
      <path {...S} d="M4.6 10h6.8" />
    </>
  ),
  // Waves.
  Soundscape: (
    <>
      <path {...S} d="M1.5 6c1.6-1.4 3.2-1.4 4.8 0s3.2 1.4 4.8 0 2.6-1.1 3.4-.6" />
      <path {...S} d="M1.5 10.5c1.6-1.4 3.2-1.4 4.8 0s3.2 1.4 4.8 0 2.6-1.1 3.4-.6" opacity="0.6" />
    </>
  ),
};

const LAYERS = (
  <>
    <path {...S} d="M8 1.6 14.4 5 8 8.4 1.6 5Z" />
    <path {...S} d="M1.6 8.2 8 11.6l6.4-3.4" />
    <path {...S} d="M1.6 11.2 8 14.6l6.4-3.4" opacity="0.6" />
  </>
);

export function hasProfileIcon(name?: string): boolean {
  return !!name && name in ICONS;
}

/** A profile's icon, in `colour`. */
export function ProfileIcon({ name, colour = "currentColor", size = 14, title }: { name?: string; colour?: string; size?: number; title?: string }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" aria-hidden={title ? undefined : true} aria-label={title} style={{ flexShrink: 0, display: "block", color: colour }}>
      {(name && ICONS[name]) ?? LAYERS}
    </svg>
  );
}
