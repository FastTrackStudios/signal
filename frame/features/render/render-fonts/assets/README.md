# Bundled fonts

Every family here is licensed under the **SIL Open Font License 1.1**; its
`OFL.txt` (copyright line and full licence) sits beside its fonts. They are
embedded into frame binaries by `render-fonts` (`src/bundled.rs`).

Fonts **without** a Reserved Font Name were subset with fontTools
(`fonttools subset --unicodes=… --layout-features='*' --no-hinting`) to
Basic Latin, Latin-1, a few Latin Extended letters (Œ œ Š š Ÿ Ž ž ƒ ı),
general punctuation, super/subscripts, € ™ ℓ Ω, arrows ← → ↑ ↓, maths
(− ∞ ≈ ≠ ≤ ≥ √ ∑ …), geometric shapes and ♭ ♮ ♯. Fonts **with** a Reserved
Font Name ship exactly as released (only the file name changed), since the
OFL reserves that name for unmodified fonts.

| Family | Files | Source | Reserved name | Treatment | Bytes |
|---|---|---|---|---|---|
| Inter | `inter/Inter-Variable.ttf`, `Inter-Italic-Variable.ttf` | rsms/inter (via google/fonts `ofl/inter`) | — | as released | 876,576 + 906,596 |
| Oswald | `oswald/Oswald-Variable.ttf` (wght 200–700) | google/fonts `ofl/oswald` | — | subset | 57,376 |
| Saira | `saira/Saira-Variable.ttf` (wdth 50–125, wght 100–900) | google/fonts `ofl/saira` | — | subset | 276,236 |
| Michroma | `michroma/Michroma-Regular.ttf` | google/fonts `ofl/michroma` | — | subset | 24,364 |
| Orbitron | `orbitron/Orbitron-Variable.ttf` (wght 400–900) | google/fonts `ofl/orbitron` | "Orbitron" | as released | 38,576 |
| Dancing Script | `dancing-script/DancingScript-Variable.ttf` (wght 400–700) | google/fonts `ofl/dancingscript` | "Dancing Script" | as released | 133,636 |
| Pacifico | `pacifico/Pacifico-Regular.ttf` | google/fonts `ofl/pacifico` | — | subset | 64,780 |
| Caveat Brush | `caveat-brush/CaveatBrush-Regular.ttf` | google/fonts `ofl/caveatbrush` | — | subset | 137,884 |
| Knewave | `knewave/Knewave-Regular.ttf` | google/fonts `ofl/knewave` | "Knewave" | as released | 32,460 |
| Playfair Display | `playfair-display/PlayfairDisplay-Variable.ttf` (wght 400–900) | google/fonts `ofl/playfairdisplay` | "Playfair Display" | as released | 300,724 |
| DSEG7 Classic | `dseg/DSEG7Classic-{Light,Regular,Bold,Italic,BoldItalic}.ttf` | keshikan/DSEG v0.46 release | "DSEG" | as released | 116,468 |
| DSEG14 Classic | `dseg/DSEG14Classic-{Regular,Bold,Italic}.ttf` | keshikan/DSEG v0.46 release | "DSEG" | as released | 86,188 |
| Doto | `doto/Doto-Variable.ttf` (ROND, wght 100–900) | google/fonts `ofl/doto` | — | subset | 286,684 |
| VT323 | `vt323/VT323-Regular.ttf` | google/fonts `ofl/vt323` | — | subset | 56,612 |
| JetBrains Mono | `jetbrains-mono/JetBrainsMono-Variable.ttf` (wght 100–800) | google/fonts `ofl/jetbrainsmono` | — | subset | 102,200 |
| UnifrakturCook | `unifraktur-cook/UnifrakturCook-Bold.ttf` | google/fonts `ofl/unifrakturcook` | "UnifrakturCook" | as released | 42,688 |
| Pirata One | `pirata-one/PirataOne-Regular.ttf` | google/fonts `ofl/pirataone` | "Pirata" | as released | 56,316 |

Total: 3.67 MB of fonts (1.78 MB of it Inter, which frame already shipped).

Not bundled: Permanent Marker and Rock Salt are Apache-2.0, not OFL —
Caveat Brush and Knewave cover marker lettering instead.
