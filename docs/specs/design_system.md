# Design System Spec

Visual identity, design tokens, shared UI primitives, and UX rules for Mask. Structure follows the Toolzy pattern: all tokens live in one Tailwind v4 `@theme` block in `src/styles.css`; all shared visuals live in `src/components/ui.tsx`. Components consume tokens by name and never hardcode colors, sizes, or shadows.

Identity reference: Travelperk-style system (user-provided DESIGN.md/tokens.json, 2026-06-11) — warm cream surfaces, near-black text, Electric Lime as the single chromatic accent, large radii, one typeface.

## Identity

Mask is a toy you use with friends, dressed like a modern analog gadget: warm paper-like surfaces, one loud lime accent, soft pill shapes. One glance answers the three live questions: am I being heard, what voice am I wearing, what is playing.

- **Light, warm, analog.** Page ground is warm cream (#f5f5eb), cards are white, text is a warmed near-black (#14140f). No pure grays; the achromatic scale stays warm (graphite, stone).
- **Electric Lime is the only chromatic accent.** Active states, CTAs, slider fills, selected tiles. Status colors (mint, coral) appear only on status; signal blue only on the keyboard focus ring. No other accents, no gradients.
- **Elevation by contrast, not shadow.** White card over cream ground reads as elevated by itself; hairline stone borders do the rest. Heavy shadow only on floating layers (modals, toasts).
- **One typeface** (OTSono stack, falls back to system sans). Hierarchy comes from size and weight (400/500 only); numbers use tabular figures instead of a second mono font.

## Design Tokens

Palette (from tokens.json): `electric-lime #beff50`, `near-black #14140f`, `warm-cream #f5f5eb`, `parchment #fafaf5`, `stone #d2d2c8`, `graphite #6e6e64`, `charcoal #30302a`, `slate-border #919183`, `signal-blue #144fcc`, `coral-alert #eb3131`, `mint-confirm #1dc479`.

Semantic layer (what components actually use):

| Token | Value | Use |
|---|---|---|
| `ground` | warm-cream | page background |
| `surface` | white | cards, dialogs |
| `raised` | parchment | interactive resting (tiles, inputs, chips) |
| `overlay` | #ececdf | hover state of raised |
| `outline` | stone | hairline borders |
| `text` / `text-dim` / `text-faint` | near-black / graphite / slate-border | text hierarchy |
| `mask` / `mask-strong` / `mask-soft` | electric-lime / #a9e83f / lime 25% | accent, hover, selected tint |
| `live` / `danger` | mint-confirm / coral-alert | status only |
| `focus` | signal-blue | keyboard focus ring only |

Type scale: caption 12, body 14, body-lg 16, subheading 20, heading 24 (Travelperk steps, app density). Radii: `lg` 8 (inputs/badges), `2xl` 18 (tiles, list rows), `3xl` 26 (cards, dialogs), `rounded-full` (buttons, pills, banner). Shadows: `shadow-card` = 1px stone ring; `shadow-pop` = ring + soft warm shadow for floating layers.

Token rules:

1. **Components reference semantic tokens** (`bg-surface`, `text-text-dim`, `bg-mask`). Raw hex in a component is a review rejection.
2. **Weights 400 and 500 only** (`font-medium` is the strongest weight in the app). No bold, no font switching.
3. **Numbers render with `tabular-nums`** (latency ms, dB, semitones, durations) so they do not jitter.
4. **Lime never carries text smaller than body on light surfaces**; text on lime is always near-black.
5. **Small caps labels** (Field labels, badges): caption size, uppercase, `tracking-[0.1em]`.

## Shared Primitives (`src/components/ui.tsx`)

| Primitive | Contract |
|---|---|
| `focusRing` | `ring-2 ring-focus` (signal blue), offset against ground. Every interactive element. |
| `Card` | `bg-surface rounded-3xl shadow-card p-5`. |
| `PanelHeader` | Subheading (weight 500) + optional right action. |
| `pill(active)` | Active: `bg-mask text-text`; rest: `bg-raised ring-outline hover:bg-overlay`. |
| `PrimaryButton` | Lime pill (`rounded-full bg-mask text-text hover:bg-mask-strong`). One per view max. |
| `GhostButton` | Transparent pill with near-black outline (`ring-1 ring-text hover:bg-raised`). |
| `DangerButton` | Coral outline pill; double-press confirmation; armed = coral fill, white text. |
| `Field` | Small-caps caption label above a control. |
| `Select` | Parchment input, stone ring, `rounded-lg`, custom chevron. |
| `Slider` | Cream track, lime fill via `--fill`, near-black thumb. Center-detent variant for bipolar values. |
| `Meter` | mint → lime above -12 dBFS → coral at clip; rAF decay + 300 ms peak-hold. |
| `StatusDot` | stone stopped, lime pulsing starting, mint running, coral error. |
| `Toast` | White card, `shadow-pop`; errors get coral text + ring and persist; info auto-dismisses 5 s. |
| `Spinner` | Near-black ring spinner. |
| `EmptyState` | Centered title + hint + one action. |
| `MonoValue` | Same typeface, `tabular-nums`, graphite. |

Feature composites (preset tile, clip tile, model row) live in their feature files, built from these primitives.

## Key Screens

- **Main window**: cream ground, three white cards (Devices 280 px column; Effects, AI Voices, Soundboard stacked right). Header carries name, StatusDot, latency readout, and the lime announcement pill when no virtual cable is installed.
- **Selected-state grammar**: active preset tile and playing clip tile are **lime-filled** with near-black text (active chip pattern); the active AI model row uses the soft lime tint. Users learn one selection language.
- **Onboarding**: full-window cream overlay, one step per screen, lime CTA advances, progress dots, skippable.
- **Dialogs** (clip editor, model import): white `rounded-3xl` card with `shadow-pop` over a near-black 40% scrim; Esc closes; no nested dialogs.

## UX Rules

1. **State is always visible, never inferred.** Pipeline state, active preset, and playing clips each have one permanent indicator; nothing important lives only in a toast.
2. **Latency honesty**: measured end-to-end ms in the header whenever the pipeline runs.
3. **Every error names the fix** ("Virtual cable not found. Open setup guide."); raw errors go to detail, never the headline.
4. **Soundboard reachable in one click at all times.**
5. **Destructive actions confirm inline** (double-press), no OS dialogs.
6. **Hover is not a requirement**: state legible without pointer interaction.
7. **Keyboard**: full tab order with the signal-blue focusRing; Space triggers focused clip; Esc closes overlays.
8. **Motion is functional only**: 120–160 ms ease-out transitions; meters on rAF; `prefers-reduced-motion` disables the pulsing dot.
9. **Contrast AA** for all text-on-surface pairs; `text-faint` only on hints/disabled.
10. **Copy tone**: short, friendly, second person, English (V1), centralized in `src/lib/strings.ts`.

## Business Rules

1. All tokens in `src/styles.css` `@theme`; no second source of truth, no inline hex.
2. All shared primitives in `ui.tsx`; a visual needed twice moves here in the same PR.
3. Lime = interaction/identity; mint/coral = status meaning only; signal blue = focus only. Never decorate with status colors.
4. Window default 960 × 640, min 800 × 560; Devices column never scrolls, right column may scroll internally.

## Edge Cases

- Long names: single line, ellipsis, full name in `title` tooltip.
- 20+ clips: soundboard grid scrolls internally.
- Windows display scaling 125–150%: layout holds (rem-based).
- No devices: EmptyState with re-scan action, never an error toast loop.

## Open Questions

- TODO: license/availability of OTSono — currently a font stack falling back to system sans; pick a licensed lookalike (e.g. a grotesque with similar width) or commit to system sans before v0.2.
- TODO: app icon/logotype in the new identity (lime mark on near-black).
