# TIWATON AI Studio — product design system

This doc applies [DESIGN_QUALITY_STANDARD.md](DESIGN_QUALITY_STANDARD.md) to this product. Where
the two disagree, the standard wins. All tokens live in one file,
[src/styles/faceplate-tokens.css](../src/styles/faceplate-tokens.css). The
[designTokens test](../src/lib/designTokens.test.js) checks contrast and finish parity on every
`npm test`.

## Identity

**A hardware broadcast faceplate.** The desk should read like a well-made physical console:

- engraved plates, keys and knob caps;
- lamps and lit glass windows for anything live;
- tight but even spacing, with no decoration.

Colour carries meaning, as it does on a real desk. There is no brand accent applied everywhere.

The design reference is the faceplate canvas (five boards: live desk, kit, setup, editor, states).
Every feature on those boards traces back to the coverage matrix. The build work is tracked in
issue 29. Defects found during the audit are in issue 28.

## Finishes

| Finish | Use | Set by |
|---|---|---|
| graphite | Default. Dark chassis for dim church sound booths | no attribute, or `data-finish="graphite"` on `<html>` |
| bone | Light chassis for daylight rooms | `data-finish="bone"` |
| auto | Follows the system setting | `data-finish="auto"` |

The finishes replace the old theme picker (decision of 19 Nov 2026). Some surfaces are dark in
every finish:

- glass windows (meters, readouts, the clock);
- lamp colours;
- knob caps;
- channel colours.

## Colour: one colour, one meaning

| Token | Meaning | Use only for |
|---|---|---|
| `led-g` | running, safe, signal present | engine running, stream live, meter safe zone, gate open |
| `led-a` | caution or armed | warnings, restart needed, meter caution zone, a lit amber key |
| `arm` / `arm-r` | first press of a two-press key | the amber (or red, for destructive keys) edge and legend of an armed key |
| `led-r` | recording, clipping, fault | the record key, clip lamps, errors |
| `orange` | processing engaged | voice-chain stages, the voice holder, mastering progress |
| `sel` / `sel-ink` | selected | the chosen bank, page, preset or mode key |
| `ch-*` | the operator's channel colour | scribble strips and fader caps, picked per strip |
| `cap-*` | the job a knob does | trim red, filter blue, dynamics green, eq cream, pan grey, sends stone |
| `chassis` `plate` `raised` `seam` | surfaces | body, plates, strips and buses, dividers |
| `legend` `muted` | printed text | labels and values; `muted` is for secondary legends only |
| `glass*` | lit windows | readouts, meters, logs, device selects |

Rules:

- Never use red for anything except recording, clipping or a fault.
- Never use green for anything except "on and healthy".
- Purple, violet, neon blue and gradients are not part of this product.
- State never relies on colour alone. A lit key also changes its fill. Lamps carry a word next to
  them, such as "running" or "restart to apply".

### Contrast

The test enforces WCAG AA in both finishes:

- 4.5:1 for every text pair;
- 3:1 for the focus ring and key outlines.

The canvas failed four of these checks, and the token file fixes them:

| Token | Canvas value | Token value | Why |
|---|---|---|---|
| graphite `key-edge` | `#666C75` | `#70767F` | the key outline on a raised strip read at 2.83:1 |
| `on-red` | white | ink `#1A1408` | white on lamp red reads at 3.49:1 |
| graphite `arm-r` | lamp red `#F0533F` | `#FF7A66` | a red legend on a graphite key read at 3.58:1 (now 4.90:1) |
| bone `arm`, `arm-r` | lamp amber and red | `#8A5A00`, `#B0281A` | on a bone key they read at 1.68:1 and 3.18:1 |

In bone, an armed key also gets a pale tinted fill (`arm-bg` `#F6E7C4`, `arm-r-bg` `#F8DDD8`). Without it, the darker legend colour sat too close to an ordinary key outline to read as armed.

The record key's red fill stays the same. Only its legend changes.

## Type

There are two typefaces. Both are bundled, so the desk renders the same offline:

- **Barlow Semi Condensed** for legends and labels;
- **IBM Plex Mono** for every number, readout and timer, always with tabular numerals.

| Token | Size | Use |
|---|---|---|
| `text-xs` | 11px | legends, lamp words, small values; the size floor |
| `text-sm` | 12px | body text in sheets and logs |
| `text-md` | 13px | plate titles, strip names, key legends |
| `text-lg` | 14px | the clock |
| `text-xl` | 16px | the brand mark |
| `text-2xl` | 20px | loudness readout |
| `text-3xl` | 24px | recorder timer, sheet headings |

Three weights: 400, 500 and 600. All labels are sentence case or lower case. Never use all-caps
labels with wide tracking.

## Shape, depth, motion

- **Radius:** 1px meters, 2px lamps, 3px keys, 4px strips and glass, 6px plates. Nothing rounder.
- **Spacing:** the 4px scale, using Tailwind's default spacing. Plates use 8px padding and strips
  6px.
- **Depth:** glass windows sit recessed (`inset-glass`). There are no drop shadows on plates.
- **Glow:** the record key's lit state (`glow-rec`) is the product's only glow.
- **Motion:**
  - meters use 60ms linear;
  - key and lamp state changes use 120 to 160ms ease-out;
  - nothing pulses or loops;
  - `prefers-reduced-motion` sets every motion token to 0ms.

## Components

| Part | Rule |
|---|---|
| Key | 24px high, 3px radius, `key` fill with `key-edge` outline. Lit states: `on-green`, `on-amber`, `on-orange`, `on-sel`, `on-red`. Focus: 2px `focus` outline, 2px offset |
| Two-press key | Engine stop, scene load, scene delete, stream, record stop, new token, reset. The first press arms the key (`arm` edge and legend); a second press within `arm-timeout` (3s) acts; otherwise it disarms |
| Lamp | 8px square LED plus a word. Off is `led-off`, never hidden |
| Glass | Dark recessed window for anything measured or live |
| Knob | Cap coloured by job, arc ring, value under it in mono 11px |
| Fader | 26px cap in the channel colour with an ink line; the scale is printed on the plate |
| Meter ladder | Green, amber and red zones with segment gaps and a peak hold |
| Info tip | Round 16px ⓘ key. Hover or focus shows the tip; click pins it. Carries every explanation |

### Where the parts live

The primitives are React components in `src/components/faceplate/`:

- `Key` and `TwoPressKey` (with its `useTwoPress` hook);
- `Lamp` and `Glass`;
- `Knob` and `Fader`;
- `MeterLadder`;
- `InfoTip`.

Their styles are in `src/styles/faceplate-parts.css`, scoped to `fp-` classes. The maths behind
them is in `src/lib/faceplateControls.js` and `src/lib/faceplateMeter.js`, both unit-tested: the
two-press timing, the knob arc and keyboard steps, and the meter scale, zones and ballistics.

To see every part in both finishes, run `npm run dev` and open `/?kit`. The kit page exists only
in development builds.

## Text restraint

A control shows its label and its state in one to three words. Explanations and how-tos go in an
ⓘ tip. The only exception is an error the operator must act on now: it stays visible in a few
words, and it names the next step ("ports 4460–4469 busy" with a "turn listening on again" key).

## Utilities

Every colour token has a Tailwind utility with an `fp-` prefix, for example:

- `bg-fp-plate`
- `text-fp-legend`
- `border-fp-seam`
- `bg-fp-led-g`

Type, font and radius tokens follow the same pattern: `text-fp-xs`, `font-fp-mono`,
`rounded-fp-key`. Components use these utilities or `var(--fp-*)`, and never a raw hex value or
`text-[Npx]`.

## Migration

The app today predates this system. A measured baseline from 19 Nov 2026, across 36 JSX files:

| Debt | Count |
|---|---|
| Hardcoded hex colours | 349 |
| Font sizes under the 11px floor (`text-[7–10px]`) | 324 |
| `uppercase` labels | 76 |
| Gradients or backdrop blur | 14 |
| Violet or purple classes | 2 |

Order of work:

1. Tokens and fonts (done). This added rules and changed nothing on screen.
2. Primitives: key, lamp, glass, knob, fader, meter, info tip. Built on tokens, with tests (done).
3. Header and engine control (done: `DeskHeader` replaced the old status bar), then the strip, then buses and loudness, then the voice module, then
   scenes, OBS and the recorder, then the setup screen, the editor and the states. Each plate ships
   as its own PR, with before and after screenshots from the running app.
4. Retire the old `[data-theme]` accent system and the amber overrides in `index.css` once no
   screen uses them, and switch `ThemeContext` to finishes.

Each PR runs the delivery gate in section 11 of the standard, and reports any item it could not
verify as unverified.
