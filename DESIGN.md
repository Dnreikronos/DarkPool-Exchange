---
version: alpha
name: DarkPool
description: Privacy-preserving DEX. Brutalist trading-terminal aesthetic — near-black canvas, single neon-lime accent, sharp edges, two-font system (Bebas Neue + IBM Plex Mono).

colors:
  primary: "#FFFFFF"
  secondary: "#5A5A72"
  tertiary: "#D4FF00"
  neutral: "#06060A"

  surface: "#06060A"
  surface-container: "#0C0C12"

  on-surface: "#FFFFFF"
  on-surface-variant: "#5A5A72"
  on-tertiary: "#06060A"

  outline: "#1C1C26"
  outline-variant: "#2E2E3E"

typography:
  display-xl:
    fontFamily: Bebas Neue
    fontSize: 148px
    fontWeight: 400
    lineHeight: 0.88
  display-lg:
    fontFamily: Bebas Neue
    fontSize: 72px
    fontWeight: 400
    lineHeight: 0.92
  display-md:
    fontFamily: Bebas Neue
    fontSize: 48px
    fontWeight: 400
    lineHeight: 0.95
  display-sm:
    fontFamily: Bebas Neue
    fontSize: 24px
    fontWeight: 400
    lineHeight: 1
  headline-md:
    fontFamily: Bebas Neue
    fontSize: 20px
    fontWeight: 400
    lineHeight: 1
    letterSpacing: 0.05em
  body-lg:
    fontFamily: IBM Plex Mono
    fontSize: 14px
    fontWeight: 400
    lineHeight: 1.85
  body-md:
    fontFamily: IBM Plex Mono
    fontSize: 12px
    fontWeight: 400
    lineHeight: 1.8
  body-sm:
    fontFamily: IBM Plex Mono
    fontSize: 11px
    fontWeight: 400
    lineHeight: 1.75
  label-lg:
    fontFamily: IBM Plex Mono
    fontSize: 11px
    fontWeight: 500
    lineHeight: 1.4
    letterSpacing: 0.15em
  label-md:
    fontFamily: IBM Plex Mono
    fontSize: 10px
    fontWeight: 500
    lineHeight: 1.4
    letterSpacing: 0.2em
  label-sm:
    fontFamily: IBM Plex Mono
    fontSize: 8px
    fontWeight: 500
    lineHeight: 1.4
    letterSpacing: 0.2em

rounded:
  none: 0px

spacing:
  unit: 4px
  xs: 4px
  sm: 8px
  md: 16px
  lg: 24px
  xl: 32px
  "2xl": 48px
  "3xl": 80px
  gutter-right: 280px
  page-x-mobile: 24px
  page-x-tablet: 48px
  page-x-desktop: 80px

components:
  button-primary:
    backgroundColor: "{colors.tertiary}"
    textColor: "{colors.on-tertiary}"
    typography: "{typography.label-lg}"
    rounded: "{rounded.none}"
    padding: 16px 32px
    height: 48px
  button-primary-hover:
    backgroundColor: "{colors.tertiary}"
  button-primary-disabled:
    backgroundColor: "{colors.outline}"
    textColor: "{colors.secondary}"

  button-ghost:
    textColor: "{colors.secondary}"
    typography: "{typography.label-lg}"
    rounded: "{rounded.none}"
    padding: 16px 32px
    height: 48px
  button-ghost-hover:
    textColor: "{colors.primary}"

  nav-bar:
    backgroundColor: "{colors.neutral}"
    rounded: "{rounded.none}"
    height: 64px
    padding: 0 48px
  nav-brand:
    textColor: "{colors.primary}"
    typography: "{typography.headline-md}"
  nav-link:
    textColor: "{colors.secondary}"
    typography: "{typography.label-lg}"
  nav-link-hover:
    textColor: "{colors.primary}"
  nav-link-active:
    textColor: "{colors.tertiary}"

  input-text:
    backgroundColor: "{colors.surface-container}"
    textColor: "{colors.primary}"
    typography: "{typography.body-md}"
    rounded: "{rounded.none}"
    padding: 12px
    height: 40px
  input-text-focus:
    backgroundColor: "{colors.surface-container}"
    textColor: "{colors.primary}"
  input-text-disabled:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.secondary}"
  input-label:
    textColor: "{colors.secondary}"
    typography: "{typography.label-md}"

  card-surface:
    backgroundColor: "{colors.surface-container}"
    textColor: "{colors.on-surface}"
    rounded: "{rounded.none}"
    padding: 20px
  card-bordered:
    textColor: "{colors.on-surface}"
    rounded: "{rounded.none}"
    padding: 20px
  card-bordered-hover:
    textColor: "{colors.on-surface}"

  modal-surface:
    backgroundColor: "{colors.surface-container}"
    textColor: "{colors.on-surface}"
    rounded: "{rounded.none}"
    padding: 32px

  divider:
    backgroundColor: "{colors.outline}"
    height: 1px
  divider-emphasis:
    backgroundColor: "{colors.outline-variant}"
    height: 1px

  step-node:
    backgroundColor: "{colors.neutral}"
    textColor: "{colors.tertiary}"
    typography: "{typography.label-md}"
    rounded: "{rounded.none}"
    width: 30px
    height: 30px

  stat-value:
    textColor: "{colors.primary}"
    typography: "{typography.display-sm}"
  stat-value-accent:
    textColor: "{colors.tertiary}"
    typography: "{typography.display-sm}"
  stat-label:
    textColor: "{colors.on-surface-variant}"
    typography: "{typography.label-md}"

  status-pill-live:
    backgroundColor: "{colors.tertiary}"
    rounded: "{rounded.none}"
    width: 6px
    height: 6px
  status-pill-offline:
    backgroundColor: "{colors.secondary}"
    rounded: "{rounded.none}"
    width: 6px
    height: 6px
  status-label-live:
    textColor: "{colors.tertiary}"
    typography: "{typography.body-sm}"
  status-label-offline:
    textColor: "{colors.secondary}"
    typography: "{typography.body-sm}"

  tag-bracketed-live:
    textColor: "{colors.tertiary}"
    typography: "{typography.label-lg}"
  tag-bracketed-static:
    textColor: "{colors.secondary}"
    typography: "{typography.label-lg}"

  ticker-bar:
    backgroundColor: "{colors.neutral}"
    textColor: "{colors.tertiary}"
    typography: "{typography.label-md}"
    rounded: "{rounded.none}"
    padding: 10px 0
    height: 36px

  terminal-feed:
    textColor: "{colors.outline-variant}"
    typography: "{typography.label-md}"
    rounded: "{rounded.none}"
    width: 260px
    padding: 16px 20px 16px 16px

  table-header:
    backgroundColor: "{colors.surface-container}"
    textColor: "{colors.on-surface-variant}"
    typography: "{typography.label-md}"
    rounded: "{rounded.none}"
    padding: 10px 16px
    height: 36px
  table-row:
    textColor: "{colors.on-surface}"
    typography: "{typography.body-sm}"
    padding: 12px 16px
  table-row-hover:
    backgroundColor: "{colors.surface-container}"
    textColor: "{colors.on-surface}"
---

## Overview

DarkPool is a privacy-preserving decentralized exchange where orders stay
cryptographically hidden until settlement. The visual identity should feel
like **infrastructure**, not a consumer app: a Bloomberg terminal crossed
with a brutalist broadsheet.

The aesthetic is hard-edged and monochromatic, punctuated by a single
neon-lime accent that marks the only thing the user needs to see in any
given moment. Every pixel is functional — no decorative gradients, no
rounded corners, no second accent color. Empty space is intentional and
heavy. Type carries the brand: condensed all-caps display copy paired with
generously tracked uppercase mono labels, both reading like signals from a
trading floor.

The emotional response should be **calm authority** — the interface of a
precision instrument. Users should feel the protocol is alive (subtle
ambient motion, blinking status indicators, scrolling tickers) but not
solicited or sold to. DarkPool states facts; it never markets.

## Colors

The palette is monochromatic by design — five neutral steps from near-black
to pure white — punctuated by a single high-luminance accent. Surfaces are
distinguished by tonal shift and 1px borders, never by shadows or radii.

- **Primary (#FFFFFF):** Pure white. Headlines, key values, hover states.
  Carries the typographic weight of the brand.
- **Secondary (#5A5A72):** Slate. Body copy, mono labels, secondary
  metadata. Contrast against the canvas is ~3:1 — WCAG AA for large text
  (18pt+ / 14pt bold+), **fails AA for normal text**. Acceptable for
  ambient labels and decorative metadata; not acceptable for primary body
  copy or any text the user must read to complete a task.
- **Tertiary (#D4FF00):** Voltage Lime. The single interaction color —
  primary CTAs, live status indicators, verified-proof markers. Used for
  **at most one element per view**.
- **Neutral (#06060A):** Inkwell canvas. A near-black with a faint blue
  undertone — warmer than charcoal, colder than pure black.
- **Surface containers** (`surface`, `surface-container`) provide a
  4-point luminance lift between the canvas and raised content.
- **Outlines** (`outline`, `outline-variant`) define every container edge
  at 1px. Default for dividers, emphasized variant for separators that
  need to read.

Semantic colors (red errors, green success) are intentionally absent.
Errors render in mono on `secondary`; success uses `tertiary`. If a chart
needs additional data channels, use opacity or luminance steps of existing
tokens — never introduce a new hue.

## Typography

Two typefaces only: **Bebas Neue** for monumental display copy, **IBM Plex
Mono** for everything else. The brand voice lives in the contrast between
the two — condensed all-caps declarations paired with monospaced whispers.

- **Display (Bebas Neue):** Used at four scales from `display-sm` (24px,
  step cards) to `display-xl` (148px, hero). Always uppercase, tight
  leading (0.88–0.95), zero tracking. A single hero word may be rendered
  as an outline (`-webkit-text-stroke: 2.5px {colors.tertiary}`) for
  emphasis — at most one outlined word per page.
- **Body (IBM Plex Mono Regular):** Three sizes — `body-lg` (14px) for
  primary descriptions, `body-md` (12px) for general copy, `body-sm`
  (11px) for dense lists. Leading ranges from 1.75 to 1.85 for comfortable
  reading at small sizes.
- **Labels (IBM Plex Mono Medium):** Three sizes from `label-sm` (8px) to
  `label-lg` (11px). **Always uppercase, always tracked** between 0.15em
  and 0.20em. This is the signature DarkPool ornament — section kickers,
  status labels, button text, metadata tags. If a label is not uppercase
  and tracked, it isn't a DarkPool label.

Headlines use the `headline-md` token only — a 20px Bebas Neue for the nav
brand wordmark. Avoid intermediate display sizes elsewhere; the system is
designed for hard hierarchy jumps, not smooth gradients.

## Layout & Spacing

Layout follows an **implicit 24px baseline** seeded by the dot-grid overlay
(see Elevation & Depth). The spacing scale is multiples of 4px, with 8px
and 24px as the dominant rhythms.

- **Horizontal page padding** steps up by breakpoint:
  `page-x-mobile` (24px) → `page-x-tablet` (48px) → `page-x-desktop` (80px).
  Apply uniformly to every section.
- **Right gutter** (`gutter-right`, 280px) is reserved on desktop for
  ambient content (terminal feed, status rails). Main column padding ends
  before this gutter; mobile drops it entirely.
- **Section rhythm** is generous: 96px (`md:py-24`) for default sections,
  128px (`md:py-32`) for hero/feature sections. Avoid tight section
  packing — DarkPool reads at a deliberate pace.
- **Component spacing** uses the scale: card padding 20px, modal padding
  32px, list-item gaps 8–16px, button padding 16×32px.

The grid is implicit — there is no formal column system. Sections are
either single-column or two-column flex/grid compositions. Resist adding a
12-column grid framework; the constraint forces clarity.

## Elevation & Depth

DarkPool uses **borders and tonal shifts, not shadows**, for hierarchy.
The interface is flat by design; every surface lives on the same plane.
Depth is communicated by:

- **Tonal shift:** Raised content sits on `surface-container` (#0C0C12)
  against the `surface` canvas (#06060A) — a four-point luminance lift.
- **1px outline:** Every container has a single-pixel border in `outline`
  (#1C1C26). Emphasis or hover uses `outline-variant` (#2E2E3E) or
  `tertiary` at 30% opacity (`border-brand-accent/30`).
- **Single allowed glow:** Primary buttons emit `box-shadow: 0 0 32px
  rgba(212, 255, 0, 0.45)` on hover. This is the **only** shadow in the
  system. No drop shadows, no inset shadows, no neumorphism.
- **Backdrop blur:** The fixed top nav uses `backdrop-blur-sm` (4px) over
  an 80%-opaque canvas. This is the only blur in the system.
- **Dot-grid overlay:** A site-wide signature treatment —
  `radial-gradient(#D4FF00 1px, transparent 1px) 24px 24px` at opacity
  0.025, fixed to the viewport with `z-index: 9999` and
  `pointer-events: none`. Faint CRT-phosphor effect that also seeds the
  spacing baseline.

## Shapes

**Corner radius is zero. Always.** Sharp edges are a brand rule, not a
default. Buttons, cards, inputs, modals, status pills, badges — every
surface has a 0px radius. The `rounded` token defines a single value
(`none: 0px`) that all components reference; it exists primarily to
communicate the rule.

The single exception is the blinking status dot, which must apply
`borderRadius: 0` explicitly to override any framework-inherited
`rounded-full` — even when the underlying primitive is round, the rendered
dot is square.

Borders, on the other hand, are universal. Every interactive surface and
every container is enclosed by a 1px `outline` line. Borders do the work
that radii would do in a softer system: they define edges, separate
content, and signal state through color shifts on hover and focus.

## Components

### Buttons

`button-primary` fills with `tertiary` (lime), text in `on-tertiary` (the
canvas color), `label-lg` mono typography. Hover emits the single allowed
glow (300ms ease). Used for exactly one action per view.

`button-ghost` is transparent with a 1px `outline-variant` border and
`secondary` text. Hover lightens the border to `secondary` and the text to
`primary`. Used for everything that isn't the single primary action.

Both use `padding: 16px 32px` (vertical × horizontal) and a 48px target
height — comfortably above the 44px touch minimum.

### Navigation

`nav-bar` is fixed to the top of the viewport, full-width, 64px tall,
80%-opaque canvas with `backdrop-blur-sm`, 1px `outline` bottom border.
The brand wordmark uses `nav-brand` (`headline-md`, white). Links use
`nav-link` (`label-lg`, `secondary`); the active route uses `tertiary`;
hover transitions to `primary` in 150ms.

Keep navigation to **at most four primary links**. Anything else belongs
inside a dedicated page, not a dropdown.

### Inputs

`input-text` rests on `surface-container` with a 1px `outline` border,
`body-md` mono typography at 40px height. Focus replaces the border with
`tertiary`. Labels use `input-label` (`label-md`, `secondary`, uppercase
tracked) and sit above the field, not floating inside it.

Native HTML inputs require custom styling to match — no rounded corners,
no system-supplied focus rings (replace with our 1px `tertiary` outline
at `outline-offset: 2px`).

### Cards & Surfaces

`card-surface` is a filled card on `surface-container` with a 1px
`outline` border and 20px padding. Use for any content block that should
read as raised.

`card-bordered` is transparent (canvas-bleed) with the same border and
padding — used when the dot-grid overlay should remain visible through
the card. Hover lifts the border to `tertiary` at 30% opacity.

`modal-surface` is `surface-container` with 32px padding, centered with a
canvas-tinted backdrop (`rgba(6, 6, 10, 0.85)`). Modals never have a
visible chrome border — the backdrop is enough separation.

### Lists & Tables

`table-header` sits on `surface-container` with `label-md` text in
`on-surface-variant`, 36px tall, 16px horizontal padding. Header cells are
uppercase tracked labels — they read as metadata, not column titles.

`table-row` uses `body-sm` text in `on-surface` with 12×16px padding. Rows
are separated by 1px `outline` bottom borders only (no zebra striping, no
vertical dividers). Hover applies the `surface-container` background.

### Status indicators

`status-pill-live` and `status-pill-offline` are 6×6px squares (square,
not round — `borderRadius: 0` is mandatory) that pair with a `body-sm`
label. The live variant blinks at 1Hz (`animate-blink`); the offline
variant is static.

`tag-bracketed-live` and `tag-bracketed-static` wrap metadata in literal
square brackets — `[ PROTOCOL v0.1 — ARBITRUM ]` — using `label-lg` mono
text in `tertiary` (live) or `secondary` (static). Bracketed tags are
how DarkPool signals "this is metadata about a system state."

### Step nodes

`step-node` is a 30×30px outlined square that contains a two-digit
zero-padded step number (`01`, `02`, …) in `label-md` text colored
`tertiary`. Use for numbered sequences (onboarding flows, protocol
explainers, status timelines).

### Stats

`stat-value` and `stat-value-accent` render a single large number in
`display-sm` (24px Bebas Neue), white or lime. Pair each with `stat-label`
(`label-md`, secondary, uppercase tracked) sitting directly below. Group
three to four stats horizontally with vertical `divider` separators.

### Ambient elements

`ticker-bar` is a full-width 36px-tall marquee with top and bottom 1px
`outline` borders, scrolling `label-md` text in `tertiary` at 30 seconds
per loop. Duplicate the content 4–8× inline so the loop never visibly
restarts.

`terminal-feed` is a fixed 260px right rail of vertically scrolling mono
text at 40% opacity, `pointer-events: none`, `aria-hidden="true"`. Used
as ambient signal that the protocol is alive — never for content the user
must read. The right page gutter (`gutter-right`, 280px) reserves space
for it on desktop.

### Motion contract

All components must honor `prefers-reduced-motion: reduce`:

- Ambient loops (ticker, terminal feed, blinking dots) freeze at their
  initial frame.
- Entrance reveals (line-reveal, scroll-triggered fades) skip to their
  final state instantly.
- Hover transitions still play — these are user-initiated, not ambient.

The motion stack is GSAP 3 with `@gsap/react`'s `useGSAP` and
`ScrollTrigger`. Entrance reveals use `power3.out` or `power4.out`,
duration 0.5–0.8s, stagger 0.07–0.10s. Ambient loops are always linear.

## Do's and Don'ts

- **Do** use `tertiary` for exactly one element per view — the single most
  important action, status, or value. Two lime elements means neither is
  special.
- **Do** keep typography to Bebas Neue (display) and IBM Plex Mono (body
  and labels). Two families, no exceptions.
- **Do** uppercase and track every label under 14px (`letterSpacing`
  between 0.15em and 0.20em). This is what makes a label feel like
  DarkPool.
- **Do** honor `prefers-reduced-motion: reduce` on every animated
  element — ambient loops freeze, entrance reveals skip to final state.
- **Do** use 1px `outline` borders to define every container edge.
  Borders carry the structural work that shadows and radii would carry in
  softer systems.
- **Do** reserve the 280px right gutter on desktop for ambient content
  (terminal feed, status rails). Keep primary content out of it.

- **Don't** introduce a non-zero corner radius anywhere. Radius is `0`,
  always. The `rounded` token defines only `none`.
- **Don't** add a second accent color. If a chart needs additional data
  channels, use opacity or luminance steps of `tertiary` and
  `primary → secondary`. Never introduce red, green, blue, or yellow.
- **Don't** use semantic colors for errors or success. Errors render as
  mono copy on `secondary`. Success uses `tertiary`.
- **Don't** use drop shadows except the single primary-button hover glow
  (`0 0 32px rgba(212, 255, 0, 0.45)`).
- **Don't** mix sentence case with display copy. Display is uppercase;
  labels are uppercase. Body copy is sentence case.
- **Don't** use marketing tone in any copy. No exclamation marks, no
  emoji, no superlatives ("revolutionary," "next-gen," "best-in-class").
  DarkPool states facts.
- **Don't** stack two `button-primary` instances side-by-side. Pair a
  primary with a ghost, never two primaries.
- **Don't** use `secondary` (#5A5A72) for text the user must read to
  complete a task. It fails WCAG AA for normal text (~3:1 against the
  canvas). Use `primary` (#FFFFFF) for any text on the critical path.
- **Don't** use icons. DarkPool has no icon library; identity comes from
  typographic ornaments (numeric step nodes, bracketed tags,
  box-drawing characters `█ ░ ─` for redaction and skeletons).
