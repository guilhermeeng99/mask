# Design System Spec

Visual identity, design tokens, shared UI primitives, and UX rules for Mask. Follows the Toolzy pattern: all tokens live in one Tailwind v4 `@theme` block in `src/styles.css`; all shared visuals live in `src/components/ui.tsx`. Components consume tokens by name and never hardcode colors, sizes, or shadows.

## Identity

Mask is a toy you use with friends. The UI should feel like a compact piece of audio gear: dark, focused, a little playful, never corporate. One glance answers the three live questions: am I being heard, what voice am I wearing, what is playing.

- **Dark theme only in V1.** Voice changers run next to games and Discord at night; light mode is a later option, the token layer already permits it.
- **Playful through color and motion, not clutter.** Layout is a calm grid; personality comes from the accent color, meter movement, and small touches (the active preset "wears the mask").
- **Density medium.** It is a control panel, not a dashboard: large click targets (soundboard mid-call is aimed fast), generous spacing, no scroll in the main window at the default size.

## Design Tokens

```css
/* src/styles.css */
@import "tailwindcss";

@theme {
  /* Surfaces (dark, slightly blue ink) */
  --color-ink:          #0e1016;   /* window background */
  --color-surface:      #161a23;   /* panels / cards */
  --color-raised:       #1e2430;   /* interactive resting (buttons, inputs, clip tiles) */
  --color-overlay:      #262d3c;   /* hover state of raised */
  --color-outline:      #2e3646;   /* hairline borders */

  /* Text */
  --color-text:         #e8ecf4;   /* primary */
  --color-text-dim:     #9aa5b8;   /* secondary, labels */
  --color-text-faint:   #5c6679;   /* disabled, placeholders */

  /* Brand */
  --color-mask:         #8b5cf6;   /* violet, primary accent: active states, CTAs, slider fill */
  --color-mask-strong:  #a78bfa;   /* hover / emphasis on dark */
  --color-mask-soft:    #8b5cf61f; /* 12% tint: selected backgrounds, focus glow */

  /* Signal (meters and status) */
  --color-live:         #34d399;   /* green: signal present, running, success */
  --color-warn:         #fbbf24;   /* amber: hot signal, degraded (CPU fallback, high latency) */
  --color-danger:       #f87171;   /* red: clipping, errors, destructive actions */

  --font-app: "Inter", ui-sans-serif, system-ui, "Segoe UI", Roboto, sans-serif;
  --font-mono: "JetBrains Mono", ui-monospace, Consolas, monospace; /* numbers: ms, dB, semitones */

  /* Type scale (mirrors Toolzy's naming) */
  --text-body: 13px;        --text-body--line-height: 1.6;
  --text-body-lg: 15px;     --text-body-lg--line-height: 1.6;
  --text-subheading: 17px;  --text-subheading--line-height: 1.5;
  --text-heading: 22px;     --text-heading--line-height: 1.35;

  /* Dark-tuned shadows: shadows barely read on dark, so elevation = border + subtle glow */
  --shadow-card: 0 0 0 1px var(--color-outline), 0 8px 24px rgba(0, 0, 0, 0.35);
  --shadow-pop:  0 0 0 1px var(--color-outline), 0 16px 40px rgba(0, 0, 0, 0.5);
}
```

Token rules:

1. **Components reference tokens by name** (`bg-surface`, `text-mask`, `shadow-card`). Raw hex in a component is a lint-review rejection.
2. **Spacing and radius use Tailwind defaults** (Toolzy rule). Radius vocabulary: `rounded-lg` controls, `rounded-2xl` cards and tiles, `rounded-full` pills and dots.
3. **Numbers are mono**: latency ms, dB values, semitone offsets, clip durations render in `--font-mono` so they do not jitter while updating.
4. **`--color-mask` is the only brand hue.** Status colors are reserved for status; never decorate with green/amber/red.

## Shared Primitives (`src/components/ui.tsx`)

Single file, Toolzy-style. Components never restyle these locally; variants are added here.

| Primitive | Contract |
|---|---|
| `focusRing` | Const string: `focus-visible:ring-2 ring-mask ring-offset-2 ring-offset-ink`. Every interactive element includes it. |
| `Card` | `bg-surface rounded-2xl shadow-card p-5`, vertical gap. Panels: Devices, Effects, Soundboard. |
| `PanelHeader` | Card title row: subheading text + optional right-side action (e.g. "Import", "Stop all"). |
| `pill(active)` | Segmented pill (preset tiles list filter, settings tabs). Active: `bg-mask text-ink`; rest: `bg-raised text-text-dim hover:bg-overlay`. |
| `PrimaryButton` | `bg-mask` filled, `hover:bg-mask-strong`, disabled 50%. One per view max (Start). |
| `GhostButton` | `bg-raised hover:bg-overlay` with outline; everything secondary. |
| `DangerButton` | Outline `text-danger`; delete clip, delete preset. Confirms via its own pressed-again state, no native dialogs. |
| `Field` | Uppercase `text-body text-text-dim tracking-wide` label above a control (Toolzy's Field). |
| `Select` | Styled native select on `bg-raised`, custom chevron. Device pickers. |
| `Slider` | Range input with `--fill` gradient trick from Toolzy: track `bg-overlay`, fill `--color-mask`, thumb with surface border. Used by pitch, formant, volume. Center-detent variant for bipolar values (pitch 0 snaps). |
| `Meter` | Vertical or horizontal level meter, driven by `pipeline://metrics`. Gradient live → warn above -12 dBFS → danger at clip; 300 ms peak-hold tick. Decays via rAF (see [app_shell.md](app_shell.md) rule 3). |
| `StatusDot` | Pipeline state: `text-faint` stopped, warn pulsing starting, live running, danger error (see app_shell rule 6). |
| `Toast` | Bottom-right stack, `shadow-pop`; danger surface for errors, surface for info. Auto-dismiss 5 s, errors persist until dismissed. |
| `Spinner` | Border-spin ring in `--color-mask` (Toolzy's Spinner, recolored). |
| `EmptyState` | Centered icon + one-line hint + one action. Used by empty soundboard and no-devices. |
| `Kbd` | Small keycap chip, reserved for the phase 2 hotkeys UI. |

Feature-specific composites (preset tile, clip tile) live in their feature folder but are built from these primitives.

## Key Screens

### Main window (single screen, see [app_shell.md](app_shell.md) layout)

- Three Cards on `bg-ink`: Devices (left column, fixed 280 px), Effects (right top), Soundboard (right bottom, grows).
- Header is part of the window chrome row: app name, `StatusDot` + state label, latency readout in mono, settings gear.
- **Preset tiles** (Effects): grid of `rounded-2xl` tiles, icon + name. Active tile: `bg-mask-soft` + `ring-1 ring-mask` + icon tinted `--color-mask`. One tile is always active (`Clean` default). Custom sliders (pitch/formant) sit under the grid in a collapsible row.
- **Clip tiles** (Soundboard): same tile grammar as presets so the app reads as one family. Playing clip shows a thin progress bar along the tile bottom in `--color-live` and swaps its icon to a stop glyph. Grid is virtualized, tiles min 96 px wide.
- **Meters**: input meter beside the mic select, output meter beside the virtual-mic status; both always visible while running. Silence + running is the "am I muted?" panic case, the input meter answers it.

### Onboarding overlay ([virtual_mic_setup.md](virtual_mic_setup.md))

Full-window `bg-ink` overlay, one step per screen, max ~60 chars per line of copy, `PrimaryButton` advances. Progress as dots, skippable per spec. The success step shows the live output meter moving as the user speaks: the design moment that proves the route works.

### Settings modal

`shadow-pop` Card over a scrim (`bg-ink/70`). Pill tabs: Audio, Models (phase 2), About. No nested modals.

## UX Rules

1. **State is always visible, never inferred.** Pipeline state, active preset, and playing clips each have exactly one permanent indicator. Nothing important lives only in a toast.
2. **Latency honesty**: the header shows measured end-to-end latency in ms (mono font) whenever the pipeline runs. If AI VC mode degrades (fallback, CPU), the StatusDot goes warn and the reason is one click away.
3. **Every error names the fix.** Toast copy pattern: what failed + what to do ("Virtual cable not found. Open setup guide."). Raw Rust errors go to a collapsible detail, never the headline.
4. **Soundboard is reachable in one click at all times**: no tabs in the main window may hide it (app_shell layout already guarantees this).
5. **Destructive actions confirm inline** (DangerButton double-press pattern), no OS dialog boxes.
6. **Hover is not a requirement**: every control's state is legible without pointer interaction (active styles, not hover-only affordances).
7. **Keyboard**: full tab order with the shared `focusRing`; Space triggers focused clip; Esc closes modal/overlay. Global hotkeys are phase 2 ([soundboard.md](soundboard.md) TODO).
8. **Motion is functional only**: 120–160 ms ease-out for state transitions (tile activation, toast slide), meters and progress run on rAF. No decorative animation loops; `prefers-reduced-motion` disables the pulsing StatusDot.
9. **Contrast**: all text-on-surface pairs meet WCAG AA (4.5:1); `text-faint` is allowed only on disabled controls.
10. **Copy tone**: short, friendly, second person, English (V1), centralized in `src/lib/strings.ts`. No jargon in headlines; "ms" and "dB" allowed in mono readouts.

## Business Rules

1. **All tokens in `src/styles.css` `@theme`**; no second source of truth, no inline hex.
2. **All shared primitives in `src/components/ui.tsx`**; a visual needed twice moves here in the same PR.
3. **The active preset and playing clip styles are the same selected-tile grammar** (`bg-mask-soft` + ring) so users learn it once.
4. **Status colors map 1:1 to meaning** (live/warn/danger) across meters, dots, toasts, and banners.
5. **Window default 960 × 640, min 800 × 560**; the three-panel layout must not scroll at default size with 12 presets and 8 clips visible.

## Edge Cases

- **Very long clip/preset names** — single line, ellipsis, full name in `title` tooltip.
- **20+ clips** — soundboard grid scrolls internally; Devices and Effects never scroll.
- **Windows display scaling 125–150%** — layout is rem-based and must hold at both.
- **No devices at all** (fresh VM, no mic) — Devices panel shows EmptyState with a re-scan action, never an error toast loop.

## Open Questions

- TODO: app icon and logotype (the "mask" mark) before v0.1.0 release.
- TODO: bundle Inter + JetBrains Mono as packaged fonts vs system fallback (installer size vs consistency).
