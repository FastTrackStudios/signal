---
target: top bar, menus, both sidebars
total_score: 21
max_score: 40
na_heuristics: 
p0_count: 0
p1_count: 3
target_identity: "file:/Volumes/build-disk/development/signal/features/rigs/guitar/ui/src"
timestamp: 2026-10-06T22-32-26Z
slug: features-rigs-guitar-ui-src
---
Method: dual-agent (A: design review · B: detector + grep)

Target: Signal guitar rig desktop chrome — top bar, menus, left sidebars (Presets/Profile/Setlist), right module sidebar, Library, palette. Screenshots at 1512x945 before the bar change that removed Routing/Tones outside Preset mode and replaced "Modules" with a right-rail toggle.

## Design Health Score
| # | Heuristic | Score | Key Issue |
|---|---|---|---|
| 1 | Visibility of System Status | 2 | Audio stopped = a 7px blinking dot; NEXT is 8px faint; dropouts only inside a menu |
| 2 | Match System / Real World | 2 | "Preset" means four things; stack, snapshot, Core, BUF, Lands on |
| 3 | User Control and Freedom | 3 | Revert/Esc/two-step delete good; one click changes rig-wide perform mode; reload has no confirm |
| 4 | Consistency and Standards | 1 | Two styling systems; ~6 "playing" looks, 4 selected-tab looks, 14 radii, 3 sidebar widths |
| 5 | Error Prevention | 2 | Menu refusals excellent; stage hazards (mode, reload, BUF) one click away in the bar |
| 6 | Recognition Rather Than Recall | 2 | Recipe empty states; double-click indicator menus; hidden second-click opens Library |
| 7 | Flexibility and Efficiency | 3 | ⌘P/⌘L/right-click; palette keycaps print "⌘ARROWRIGHT"; menus not keyboard-driven |
| 8 | Aesthetic and Minimalist Design | 2 | ~22 equal-weight bar controls; ⋯ on every row; repeated "Deluxe + AC30" meta |
| 9 | Error Recovery | 2 | Every rig call is fire-and-forget (`let _ = r.x().await`); refusals never shown |
| 10 | Help and Documentation | 2 | Tooltips only (dead on iPad); 10px faint prose footers |
| **Total** | | **21/40** | **Acceptable** |

## Design Specificity Verdict
Content is authored for Signal (setlist timeline, stack-tinted patch chips, refusals with reasons, "Playing:" context); the visual form is a generic dark zinc pro-tool kit and is tuned for prep, not the stage. Deterministic scan: the detector skips .rs; extracted rsx->HTML gave 131 findings (62 undersized-ui-text, 29 tiny-text, 14 low-contrast, 4 cramped-padding, 3 side-tab, 13 thin-border-wide-shadow advisories, 6 em-dash advisories). Grep: 37 clickable elements with no hover, 30 hex colours outside theme.rs, 14 radius values, selected state in two systems. Detector-only catches: 2px coloured left edges with radius (setlist_bar.rs:716, sound_browser.rs:623), "FX off" #fff on #ec4899 = 3.5:1. False positives: module_sidebar.rs:697 contrast (merged branches), cramped-padding on a meter track and icon buttons.

## Priority Issues
- [P1] Audio stopped / recovery hidden — indicators.rs. Fix: single-click opens via PopupHost, no mouseleave close; stopped = red pill "Audio stopped · Start"; drops shown in bar. (harden)
- [P1] Rig-wide perform mode looks like a local view tab; second click secretly opens Library; Library uses the Presets icon; reload + BUF are stage hazards in the bar — remote.rs. Fix: distinct segmented control with LIVE accent, labelled Library, move reload/BUF to the Audio menu + palette. (distill, clarify)
- [P1] No shared state vocabulary: playing/selected/hover/tabs/radii differ per surface; Profile tree is Tailwind (breaks inline-only rule) — sidebars.rs, preset_bar.rs, module_sidebar.rs, kit.rs. Fix: shared primitives (SidebarHeader, ListRow states, MenuPanel, SegmentedTabs, Button, SearchField, EmptyState) + one hover value; port Profile tree to ListRow. (extract, polish)
- [P2] Type and contrast below the floor: 91 sub-11px findings; FAINT #63636b 3.2:1; DIM used as text 1.8:1; NEXT 8px. Fix: 10px eyebrow / 11px floor, FAINT -> ~#8b8b94. (typeset, audit)
- [P2] Right sidebar mixes levels (7 flat tabs), duplicate ⋯ menu, clipped card name, invisible search placeholder, local 232px width; menus overflow the window edge. Fix: [Preset|Core|Time] + sub-chips, drop header ⋯, measured clamp, "In use: A, B +3 more". (distill, harden)

## Persona Red Flags
- Alex: hidden second-click Library; double-click indicator; raw keycap names; no keyboard menus.
- Sam: div-onclick rows, outline:none everywhere, colour-only state, 3.2:1 / 1.8:1 text.
- Gig player (1.5 m, dark): chrome illegible at distance; brightest chrome element is the white "Core" editing tab; 7px audio dot; hazards in the status strip.

## Minor Observations
Radii 2-14; widths 272/256/232; four "+ New" styles; eyebrow variants; off-palette literals in remote.rs/indicators.rs; names clipped with no ellipsis; Presets empty state is a click-path recipe; palette ACTION/CREATE column is noise; "→ Core on this patch" looks like a link but isn't.

## Questions to Consider
- Should a stage layout collapse the chrome to song · patch · audio · tuner at 2-3x type?
- Is perform mode a bar tab at all, or a property of what's loaded?
- Song -> Part -> Patch -> Sound instead of preset/stack/snapshot?
