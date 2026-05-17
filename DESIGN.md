# DarkPool Design System

> Visual language for the DarkPool Exchange — a decentralized exchange where
> orders stay private until settlement. This document is the source of truth
> for color, type, layout, motion, components, patterns, voice, and
> accessibility across every surface (landing, app, dashboards, docs).
>
> Structure follows Google's Material Design documentation pattern:
> **Foundations → Components → Patterns**, preceded by principles and closed
> with voice/tone and accessibility.

---

## 1. Principles

Five rules. They override everything below. If a token or component conflicts
with a principle, the principle wins.

### 1.1 Zero ornament
Every pixel earns its place. No decorative shadows, no rounded corners, no
gradients, no glyphs that aren't doing structural work. A line is a divider,
not a flourish. Empty space is a feature.

### 1.2 One accent, never two
`#D4FF00` (neon lime) is the only chromatic color on the page. It marks the
single most important thing in any given view — a CTA, a successful proof, a
live status. If two things are lime, neither is special. The rest of the UI
is monochrome (`#06060A → #FFFFFF`).

### 1.3 Type carries the brand
Two typefaces only: `Bebas Neue` for display, `IBM Plex Mono` for everything
else. No serifs, no sans-serif body fonts, no decorative weights. The brand
voice lives in the **uppercase tracked mono labels** (`[ PROTOCOL v0.1 ]`)
and the **massive condensed display** (`TRADE WITHOUT REVEALING ANYTHING.`).

### 1.4 Sharp edges
`border-radius` is `0`. Always. The Tailwind config explicitly resets the
radius scale to an empty object — that's intentional. Rounded corners belong
to consumer products; DarkPool is infrastructure.

### 1.5 Motion reveals; it never decorates
Motion exists to reveal hierarchy (line-reveal on hero), communicate state
(blinking status dot, scrolling ticker), or guide the eye on scroll. Motion
never plays for its own sake. If you remove the animation and the meaning
survives, the animation was wrong.

---

## 2. Foundations

### 2.1 Color

#### 2.1.1 Token table

| Token            | Hex       | Role                                   | On-bg contrast | Notes                              |
| ---------------- | --------- | -------------------------------------- | -------------- | ---------------------------------- |
| `brand.bg`       | `#06060A` | Page background, default canvas        | —              | Near-black with a blue tint        |
| `brand.surface`  | `#0C0C12` | Raised surfaces (cards, modals)        | —              | One step up from `bg`              |
| `brand.border`   | `#1C1C26` | Default dividers, card outlines        | 1.22 : 1       | Decorative only, not for text      |
| `brand.border2`  | `#2E2E3E` | Emphasized borders, tertiary text      | 1.76 : 1       | Use sparingly — pre-disabled state |
| `brand.muted`    | `#5A5A72` | Secondary text, mono labels            | 4.51 : 1       | Passes WCAG AA for 12 px+ text     |
| `brand.accent`   | `#D4FF00` | The single brand color                 | 16.66 : 1      | CTAs, success, live indicators     |
| `white`          | `#FFFFFF` | Primary text, display copy             | 19.83 : 1      | AAA on `bg`                        |

All contrast ratios are calculated against `brand.bg` (`#06060A`). Numbers are
informational — verify with a tool (axe, Stark) before shipping copy at sizes
not yet covered by this document.

#### 2.1.2 Usage matrix

| Element                  | Token              |
| ------------------------ | ------------------ |
| Page background          | `brand.bg`         |
| Card / panel fill        | `brand.surface` or `brand.bg / 40%` over canvas |
| Default border           | `brand.border`     |
| Hover / focused border   | `brand.accent / 30%` (e.g. `border-brand-accent/30`) |
| Body copy                | `brand.muted`      |
| Headings, key values     | `white`            |
| CTA fill                 | `brand.accent`     |
| CTA text on accent fill  | `brand.bg`         |
| Status: live / success   | `brand.accent`     |
| Status: offline / idle   | `brand.muted`      |

#### 2.1.3 Forbidden

- **Gradients.** None. The dot-grid overlay is a single solid color over a
  solid background — that's not a gradient.
- **Color tints outside the seven tokens.** No new grays, no second accent.
  If a chart needs more data channels, see §7 Open Questions.
- **Opacity to fake a new color.** `brand.muted` and `brand.border2` already
  fill that role. Exception: ambient layers like the dot-grid overlay
  (`opacity: 0.025`) and right-rail terminal feed (`opacity: 0.40`) — these
  are deliberate ambient signals, not text or controls.
- **Semantic colors (red/green/blue/yellow).** DarkPool has no error red and
  no success green. Errors are mono copy on the muted track; success is the
  accent. If you reach for red, you're solving the wrong problem.

### 2.2 Typography

#### 2.2.1 Two-font system

| Family            | Weight | Variable             | Role                              |
| ----------------- | ------ | -------------------- | --------------------------------- |
| Bebas Neue        | 400    | `--font-bebas`       | Display headings, big numerics    |
| IBM Plex Mono     | 400    | `--font-ibm-plex-mono` | Body, labels, captions, code    |
| IBM Plex Mono     | 500    | `--font-ibm-plex-mono` | Emphasized labels, CTAs           |

Loaded in `front/app/layout.tsx` via `next/font/google` with `display: swap`.
Both fonts expose CSS variables consumed by Tailwind's `fontFamily.display`
and `fontFamily.mono`.

#### 2.2.2 Type scale

| Role                  | Family   | Size                          | Leading  | Tracking     | Case   |
| --------------------- | -------- | ----------------------------- | -------- | ------------ | ------ |
| Hero headline         | display  | `clamp(64px, 11vw, 148px)`    | `0.88`   | `0`          | UPPER  |
| Section title         | display  | `clamp(40px, 5vw, 72px)`      | `0.92`   | `0`          | UPPER  |
| Footer brand          | display  | `clamp(32px, 4vw, 48px)`      | `0.95`   | `0`          | UPPER  |
| Stat value (hero)     | display  | `38px`                        | `1.0`    | `0`          | UPPER  |
| Guarantee value       | display  | `clamp(36px, 4vw, 56px)`      | `1.0`    | `0`          | UPPER  |
| Step card title       | display  | `24px`                        | `1.0`    | `0`          | UPPER  |
| Nav brand wordmark    | display  | `20px`                        | `1.0`    | `0.05em`     | UPPER  |
| Body copy             | mono     | `14px`                        | `1.85`   | `0`          | sentence |
| Description           | mono     | `12px`                        | `1.80`   | `0`          | sentence |
| Step body             | mono     | `11px`                        | `1.75`   | `0`          | sentence |
| Caption / status      | mono     | `11px`                        | `1.4`    | `0`          | sentence |
| Label (primary)       | mono     | `10–11px`                     | `1.4`    | `0.15em`     | UPPER  |
| Label (kicker)        | mono     | `10px`                        | `1.4`    | `0.20–0.30em`| UPPER  |
| Microlabel / tech tag | mono     | `8–9px`                       | `1.4`    | `0.20em`     | UPPER  |

#### 2.2.3 Display patterns

**Outlined display word.** A single word in a hero or section title can be
rendered as an outline to add hierarchy without adding a color:

```tsx
<span
  className="text-transparent"
  style={{ WebkitTextStroke: '2.5px #D4FF00' }}
>
  REVEALING
</span>
```

Use sparingly — at most one outlined word per page. The stroke width matches
the typeface's stem weight at the hero size; scale it proportionally for
smaller displays (~`1.5px` at 48 px, ~`1px` at 24 px).

**Line-reveal mask.** Wrap each display line in a `overflow: hidden` div so
GSAP can translate the inner span up from below the mask. See §2.4.2 for the
motion spec.

#### 2.2.4 Mono label pattern

The signature DarkPool label:

```tsx
<span className="font-mono text-[10px] text-brand-accent tracking-[0.3em] uppercase">
  HOW IT WORKS
</span>
```

Three variants by emphasis: `brand.accent` for section kickers and key
metadata, `white` for active state, `brand.muted` for everything else.

### 2.3 Layout & Spacing

#### 2.3.1 Baseline grid

The site has an implicit **24 px baseline** anchored by the dot-grid overlay
(see §2.5.4). All vertical rhythm should land on multiples of 4 px and prefer
multiples of 8 px / 24 px where possible.

#### 2.3.2 Horizontal padding scale

| Breakpoint | Token     | Px equivalent |
| ---------- | --------- | ------------- |
| mobile     | `px-6`    | 24            |
| `md:`      | `md:px-12`| 48            |
| `lg:`      | `lg:px-20`| 80            |

Apply consistently to every full-width section. Combine with the right
gutter (§2.3.3) when ambient content needs reserved space.

#### 2.3.3 Right gutter for ambient content

The `TerminalFeed` component occupies a **260 px** fixed right rail. Sections
that should not flow under it reserve the space with `md:pr-[280px]` (hero)
or `md:pr-[300px]` (denser sections). Mobile drops the rail entirely
(`hidden md:block` on the rail itself).

#### 2.3.4 Section vertical rhythm

| Context                          | Vertical padding  |
| -------------------------------- | ----------------- |
| Section, dense                   | `py-16 md:py-20`  |
| Section, default                 | `py-24 md:py-32`  |
| Section, hero (fills viewport)   | `h-screen`        |
| Block-within-section separation  | `mb-24`           |
| Item separation (cards in list) | `mb-8 lg:mb-10`   |

#### 2.3.5 Border radius

`borderRadius: {}` — the Tailwind theme explicitly empties the radius scale.
Sharp corners are a brand rule (§1.4). The only exception is the
`rounded-full` status dot in the footer — and even that uses
`borderRadius: 0` inline to force a square pill. Don't reintroduce radii.

#### 2.3.6 Borders

| Token             | Use                                            |
| ----------------- | ---------------------------------------------- |
| `brand.border`    | Default dividers, card outlines (1 px)         |
| `brand.border2`   | Subtle decorative emphasis, separators        |
| `brand.accent/30` | Hover state on bordered cards (`hover:border-brand-accent/30`) |

Always 1 px. Never inset, outset, dashed, or dotted.

### 2.4 Motion

#### 2.4.1 Stack

- `gsap@^3.14` core
- `@gsap/react@^2.1` for `useGSAP` (handles scope + cleanup on unmount)
- `gsap/ScrollTrigger` for scroll-driven entrances

Every animated component declares `gsap.registerPlugin(useGSAP, ScrollTrigger)`
at module scope and scopes its tweens with `useGSAP(() => {...}, { scope: ref })`.

#### 2.4.2 Three motion modes

**Mode A — Entrance reveal.** Played once when an element enters the viewport
(or on initial mount for above-the-fold).

| Property      | Default                                   |
| ------------- | ----------------------------------------- |
| Translate     | `y: 20` (small), `y: 40` (medium), `y: 60` (hero lines) |
| Opacity       | `0 → 1`                                   |
| Duration      | `0.5 – 0.8s`                              |
| Ease          | `power2.out` (small) → `power4.out` (hero lines) |
| Stagger       | `0.07 – 0.10s`                            |
| Trigger       | `ScrollTrigger { start: 'top 78–85%' }`   |

The signature **line-reveal** for hero copy: wrap each display line in a
`overflow: hidden` container, then `gsap.from('.line-reveal', { y: 60, opacity: 0, stagger: 0.1, ease: 'power4.out' })`.

**Mode B — Ambient loop.** Continuous, linear, never starts or stops. Used
to communicate "the protocol is alive."

| Animation         | Duration | Ease  | Iteration |
| ----------------- | -------- | ----- | --------- |
| `marquee`         | `30s`    | linear| infinite  |
| `terminal-scroll` | `22s`    | linear| infinite  |
| `blink`           | `1s`     | ease-in-out | infinite |

Defined as keyframes in `tailwind.config.ts` and consumed via
`animate-marquee`, `animate-terminal-scroll`, `animate-blink` utility
classes.

**Mode C — Scroll-triggered choreography.** A timeline of entrance reveals
gated by a single `ScrollTrigger`. Used to sequence step cards in
`HowItWorks` (stagger 0.1, x: 30 instead of y, ease `power3.out`).

#### 2.4.3 Reduced motion contract

Respect `prefers-reduced-motion: reduce`. Implementation:

```ts
useGSAP(() => {
  const mm = gsap.matchMedia()
  mm.add('(prefers-reduced-motion: no-preference)', () => {
    // Full entrance reveal here
  })
  mm.add('(prefers-reduced-motion: reduce)', () => {
    // Set final state immediately, no animation
    gsap.set('.line-reveal', { y: 0, opacity: 1 })
  })
}, { scope: ref })
```

For ambient loops (Mode B), wrap the consumer in a media query that disables
the `animation` property under `prefers-reduced-motion: reduce`. The current
landing does **not** implement this — flagged in §6.

### 2.5 Iconography & Ornament

DarkPool has no icon library. Identity comes from typographic ornaments.

#### 2.5.1 Numeric step indicators

Use two-digit zero-padded numbers in a square `30 × 30 px` outlined box:

```
┌──┐
│01│
└──┘
```

```tsx
<div className="w-[30px] h-[30px] border border-brand-border bg-brand-bg flex items-center justify-center">
  <span className="font-mono text-[10px] text-brand-accent">01</span>
</div>
```

#### 2.5.2 Bracketed tags

Metadata labels — protocol versions, environments, build tags — wrap in
square brackets with em-dash separators:

```
[ PROTOCOL v0.1 — ARBITRUM ]
[ TESTNET — LIVE ]
```

Font: mono, `11px`, `tracking-[0.20em]`, color `brand.accent` for live tags,
`brand.muted` for static metadata.

#### 2.5.3 Box-drawing characters

ASCII / Unicode box-drawing is the closest thing DarkPool has to imagery.
Used in `TerminalFeed` to communicate "data exists but is hidden":

| Character | Use                                  |
| --------- | ------------------------------------ |
| `─`       | Horizontal rules in terminal blocks  |
| `█`       | Filled redaction blocks              |
| `░`       | Patterned redaction (hidden price)   |
| `✓`       | Proof verified marker (accent color) |

#### 2.5.4 Dot-grid overlay (signature)

Fixed, full-viewport, behind everything interactive:

```css
body::before {
  content: '';
  position: fixed;
  inset: 0;
  background-image: radial-gradient(#D4FF00 1px, transparent 1px);
  background-size: 24px 24px;
  opacity: 0.025;
  pointer-events: none;
  z-index: 9999;
}
```

The `z-index: 9999` puts it above content (so it darkens the page faintly
like a CRT phosphor) while `pointer-events: none` keeps it inert. The 24 px
spacing seeds the baseline grid (§2.3.1).

#### 2.5.5 Crosshair cursor (signature)

`cursor: crosshair !important` is applied to every element. This is a brand
choice — DarkPool is a precision instrument. See §6.3 for the a11y caveat.

---

## 3. Components

Each component lists its **anatomy**, **tokens**, **states**, a minimal
**code reference**, and **do / don't**. Snippets are the canonical
implementations from the landing source.

### 3.1 Button — Primary

The single highest-emphasis action on a view. Lime fill, dark text.

**Anatomy:** rectangular fill, no border, no radius, mono label in uppercase.

**Tokens:** `bg-brand-accent` · `text-brand-bg` · `font-mono text-xs font-medium` · `px-8 py-4`.

**States:**
- Default: as above
- Hover: glow via `hover:shadow-[0_0_32px_rgba(212,255,0,0.45)]`, 300 ms ease
- Focus: 1 px `brand.accent` outline at `outline-offset: 2px` (**not yet implemented** — see §6.4)
- Disabled: `bg-brand-border` + `text-brand-muted` + `pointer-events: none`

```tsx
<button className="bg-brand-accent text-brand-bg font-mono text-xs font-medium px-8 py-4 transition-shadow duration-300 hover:shadow-[0_0_32px_rgba(212,255,0,0.45)]">
  ENTER APP
</button>
```

**Do** use exactly one primary button per view.
**Don't** put two primaries next to each other; pair with §3.2 Ghost.

### 3.2 Button — Ghost (bordered)

Secondary actions. Outlined, mono text, no fill.

**Anatomy:** 1 px border, transparent fill, mono uppercase label.

**Tokens:** `border border-brand-border2` · `text-brand-muted` · `font-mono text-xs` · `px-8 py-4`.

**States:**
- Default: as above
- Hover: `hover:border-brand-muted hover:text-white` (border lightens, label whitens)
- Focus: same recipe as §3.1
- Disabled: `opacity-50 pointer-events-none`

```tsx
<button className="border border-brand-border2 text-brand-muted font-mono text-xs px-8 py-4 hover:border-brand-muted hover:text-white transition-colors">
  READ DOCS
</button>
```

**Do** pair with a primary, gap `gap-4`.
**Don't** stack three ghosts in a row — use a single link list instead.

### 3.3 Nav

Fixed top navigation. Blurred background, mono links.

**Anatomy:**
```
┌──────────────────────────────────────────────┐
│ DARKPOOL          PROTOCOL  DOCS  LAUNCH APP │
└──────────────────────────────────────────────┘
```

**Tokens:**
- Container: `fixed top-0 left-0 right-0 z-50 px-12 py-4 bg-brand-bg/80 backdrop-blur-sm border-b border-brand-border`
- Brand: `font-display text-[20px] text-white tracking-wider`
- Link: `font-mono text-[11px] text-brand-muted tracking-[0.15em]`
- Active link (CTA): `text-brand-accent`

**States:**
- Link hover: `hover:text-white` (150 ms)
- Active route: `text-brand-accent`

```tsx
<nav className="fixed top-0 left-0 right-0 z-50 flex items-center justify-between px-12 py-4 bg-brand-bg/80 backdrop-blur-sm border-b border-brand-border">
  <span className="font-display text-[20px] text-white tracking-wider">DARKPOOL</span>
  <div className="flex gap-6">
    <a className="font-mono text-[11px] text-brand-muted tracking-[0.15em] hover:text-white transition-colors duration-150">PROTOCOL</a>
    <a className="font-mono text-[11px] text-brand-muted tracking-[0.15em] hover:text-white transition-colors duration-150">DOCS</a>
    <a className="font-mono text-[11px] text-brand-accent tracking-[0.15em] hover:text-white transition-colors duration-150">LAUNCH APP</a>
  </div>
</nav>
```

**Do** keep at most four link items.
**Don't** add icons, badges, or dropdowns — flatten to a sub-page instead.

### 3.4 Stat Block

A vertical pairing of a display value with a mono kicker. The component
behind the hero's `100,000 ORDERS/SEC` strip.

**Anatomy:**
```
100,000
ORDERS/SEC
```

**Tokens:**
- Value: `font-display text-[38px]` (`text-brand-accent` for the lead stat, `text-white` for the rest)
- Label: `font-mono text-[10px] text-brand-muted tracking-[0.15em] uppercase mt-1`

```tsx
<div>
  <div className="font-display text-[38px] text-brand-accent">100,000</div>
  <div className="font-mono text-[10px] text-brand-muted tracking-[0.15em] uppercase mt-1">ORDERS/SEC</div>
</div>
```

**Do** group 3–4 stats in a horizontal flex with `gap-16`.
**Don't** mix display sizes in the same group — uniformity reads as
authority.

### 3.5 Step Card

Numbered node + bordered content block. Used in timelines (§4.3).

**Anatomy:**
```
┌──┐  ┌──────────────────────────────────────┐
│01│  │ COMMIT                  PEDERSEN COM │
└──┘  │ Pedersen commitment generated...     │
      └──────────────────────────────────────┘
```

**Tokens:**
- Node (`30 × 30 px`): `w-[30px] h-[30px] border border-brand-border bg-brand-bg` + centered `font-mono text-[10px] text-brand-accent` number
- Card: `border border-brand-border p-5 bg-brand-bg/40`
- Title: `font-display text-[24px] text-white`
- Tech tag: `font-mono text-[8px] text-brand-muted tracking-[0.2em]`
- Body: `font-mono text-[11px] text-brand-muted leading-[1.75]`

**States:**
- Hover: `hover:border-brand-accent/30` (300 ms color transition)

**Do** sit cards on a vertical rule (see §4.3) to imply sequence.
**Don't** use cards in isolation — they're built for lists.

### 3.6 Ticker (marquee)

Edge-to-edge horizontal scroller for protocol-status pill text.

**Anatomy:** thin band, top and bottom borders, mono accent text, infinite
horizontal scroll.

**Tokens:**
- Container: `border-b border-t border-brand-border py-2.5 overflow-hidden w-screen`
- Text: `font-mono text-[10px] text-brand-accent tracking-[0.15em]`
- Animation: `animate-marquee` (30 s linear infinite, defined in
  `tailwind.config.ts`)

```tsx
<div className="border-b border-t border-brand-border py-2.5 overflow-hidden w-screen">
  <div className="flex whitespace-nowrap animate-marquee">
    <span className="font-mono text-[10px] text-brand-accent tracking-[0.15em]">
      ZK-SNARK VERIFIED · BATCH SETTLEMENT · 100K ORDERS/SEC · P99 < 1MS · MEV EXPOSURE: ZERO ·{' '}
    </span>
  </div>
</div>
```

**Do** duplicate the text content 4–8× inline so the loop never shows a gap.
**Don't** animate this if `prefers-reduced-motion: reduce` (current source
**does not** honor this — flagged §6.1).

### 3.7 Terminal Feed (ambient right-rail)

Fixed 260 px right-rail of vertically scrolling mock terminal output. It is
ambient signal, never interactive.

**Anatomy:** fixed column, mono text right-aligned, very low opacity, no
pointer events.

**Tokens:**
- Container: `fixed top-0 right-0 bottom-0 w-[260px] overflow-hidden pointer-events-none z-[1] hidden md:block`
- Inner: `animate-terminal-scroll font-mono text-[10px] leading-relaxed text-brand-border2 opacity-40 pr-5 pl-4 pt-4 text-right`
- Accent within: proof verification marker `text-brand-accent ✓`

**Do** use box-drawing redaction (`█`, `░`) inside the feed (§2.5.3).
**Don't** make this content informationally critical — it's atmosphere.

### 3.8 Status Pill

Tiny blinking square + mono label. Communicates live/online state.

**Anatomy:** `1.5 × 1.5 px` (`w-1.5 h-1.5`) accent square that blinks every
second, paired with a mono label.

**Tokens:**
- Dot: `w-1.5 h-1.5 bg-brand-accent animate-blink inline-block` with
  `borderRadius: 0` inline override
- Label: `font-mono text-[11px] text-brand-muted` (offline) or
  `text-brand-accent` (live)

```tsx
<div className="flex items-center gap-3">
  <span className="w-1.5 h-1.5 bg-brand-accent animate-blink inline-block" style={{ borderRadius: 0 }} />
  <span className="font-mono text-[11px] text-brand-accent">Testnet: Live</span>
</div>
```

**Do** use to mark real-time states (network, deployment, build).
**Don't** use to indicate user-action success — use the primary button glow
or a toast component instead.

---

## 4. Patterns

Patterns are the recipes that combine components and tokens into the
recognizable DarkPool surfaces.

### 4.1 Hero pattern

```
┌──────────────────────────────────────── Ticker ────┐
│                                                    │
│  [ PROTOCOL v0.1 — ARBITRUM ]                      │
│                                                    │
│  TRADE WITHOUT                                     │
│  REVEALING ANYTHING.                               │  ← stroke outline on REVEALING
│                                                    │
│  Orders are cryptographic commitments.             │
│  The engine never sees price, pair or size.        │
│  Settlement is verified. Nothing is revealed.      │
│                                                    │
│  [ENTER APP]   [READ DOCS]                         │
│                                                    │
├────────────────────────────────────────────────────┤
│ 100,000   <1ms    256       0.05%                  │
│ ORDERS    P99     ORDERS    PROTOCOL               │
│ /SEC      LAT.    /BATCH    FEE                    │
└────────────────────────────────────────────────────┘
```

**Composition:**
1. `Ticker` flush to top, edge-to-edge
2. Bracketed protocol tag (§2.5.2), `mb-6`
3. Two-line display headline (line-reveal animated, §2.4.2)
4. Three-line mono description, `max-w-[480px]`
5. Primary + ghost CTA pair
6. Bottom stat bar pinned via `flex-1` content + `flex-shrink-0` strip

**Background:** WebGL `kineticGrid` shader canvas (z-0), copy and stats sit
on z-10. Right gutter `md:pr-[280px]` reserves space for the `TerminalFeed`.

**Vertical extent:** `h-screen` exactly. Hero never scrolls internally.

### 4.2 Section header

Every non-hero section starts with the same three-element header.

```
HOW IT WORKS                                          ← kicker (accent, tracked)
FOUR STEPS,                                           ← display title
ZERO KNOWLEDGE.                                       ← second line, accent

Each step is cryptographically isolated...            ← mono description, max-w-[380px]
```

**Tokens:**
- Kicker: `font-mono text-[10px] text-brand-accent tracking-[0.3em] mb-6`
- Title: `font-display text-[clamp(40px,5vw,72px)] leading-[0.92] text-white mb-8`
- Title accent line: change one phrase to `text-brand-accent`
- Description: `font-mono text-[12px] text-brand-muted leading-[1.8] max-w-[380px]`

### 4.3 Timeline pattern

Used for sequential, numbered content (the `HowItWorks` 4-step protocol
flow).

**Layout:** `grid lg:grid-cols-[1fr_1fr] gap-16 lg:gap-24`. Left column is a
**sticky** section header (`lg:sticky lg:top-32 lg:self-start`). Right column
is the step list with a `1 px` vertical rule running through the step nodes
(`absolute left-[15px] top-4 bottom-4 w-px bg-brand-border`).

Each row is a §3.5 Step Card. Stagger entrance via Mode C scroll-trigger
(§2.4.2): `x: 30 → 0`, stagger `0.1`, ease `power3.out`, trigger
`'top 78%'`.

**Do** keep the count between 3 and 6 steps.
**Don't** add a horizontal variant — DarkPool's timelines are always
vertical, mobile and desktop alike.

### 4.4 Stat bar pattern

Three equal columns with vertical dividers, used to close a section with a
"guarantees" or "metrics" summary.

```
┌──────────────┬──────────────┬──────────────┐
│ ZERO         │ NONE         │ 256          │
│ DATA LEAKED  │ MEV EXPOSURE │ TRADES / TX  │
│ Private by   │ No visible   │ Aggregated   │
│ default      │ mempool      │ proofs       │
└──────────────┴──────────────┴──────────────┘
```

**Tokens:**
- Container: `border-t border-brand-border pt-10`, preceded by a kicker
  label (`PROTOCOL GUARANTEES`, §2.2.4)
- Item: `md:border-r md:border-brand-border md:pr-8 md:pl-8`, no border on
  the last item
- Value: `font-display text-[clamp(36px,4vw,56px)] text-brand-accent leading-none`
- Label: `font-mono text-[9px] text-brand-muted tracking-[0.2em] mt-2`
- Sub: `font-mono text-[11px] text-brand-border2 mt-1`

### 4.5 Footer pattern

Brand block (left) + three link columns (center) + status row (bottom).

**Top row:**
- Brand: 2-line display wordmark with one line in `brand.accent`, followed
  by a mono description (`max-w-[320px]`)
- Three flex-wrapped link columns: `PROTOCOL` / `COMMUNITY` / `DEVELOPERS`,
  each with an accent kicker and `space-y-3` mono links

**Bottom bar:**
- 1 px top border, smaller vertical padding (`py-5`)
- Copyright + em-dash separator + two §3.8 status pills (mainnet, testnet)

Right gutter reserved (`md:pr-[300px]`) — TerminalFeed continues past the
footer until the page ends.

### 4.6 Empty & loading states

DarkPool's tone extends to non-content states. Recipes:

**Empty state:**
```
─────────────────
NO ORDERS YET
─────────────────
Submit a commit to begin.
```
- Horizontal rules above and below (`─` characters or `border-t`/`border-b`)
- Display heading at section-title scale
- Mono description in `brand.muted`

**Loading state:**
- Never use spinners. Use a §3.6 marquee or a `█████░░░░░` progress bar
  built from box-drawing characters.
- For longer waits, animate the `█` glyphs sliding right via
  `animate-marquee` at a slow speed (60 s).

**Skeleton state:**
- Replace text with `█` blocks at the same `font-size` / `leading` as the
  final content. No shimmer, no opacity pulse — static box-drawing only.

---

## 5. Voice & Tone

### 5.1 Display copy

- **ALL CAPS, declarative, ≤ 4 words per line.**
- Single-sentence ideas only. No commas, no "and."
- Prefer terms drawn from cryptography and trading floors: *commit, prove,
  match, settle, hidden, batch, mempool, proof.*
- One outlined word per page allowed (§2.2.3) for emphasis.

Examples:
- ✅ `TRADE WITHOUT REVEALING ANYTHING.`
- ✅ `FOUR STEPS, ZERO KNOWLEDGE.`
- ✅ `DARKPOOL PROTOCOL`
- ❌ `Welcome to the future of trading!`

### 5.2 Body copy

- **Mono, sentence case, ≤ 3 short sentences per block.**
- Technical specificity beats prose. Use absolute units (`P99 < 1ms`,
  `256 orders/batch`, `0.05% protocol fee`), never relative claims
  ("incredibly fast," "industry leading").
- One verb per sentence. Subject + verb + object, period.
- No contractions (`do not` over `don't`) in product copy. Engineering docs
  may use contractions.

Examples:
- ✅ `Orders are cryptographic commitments. The engine never sees price,
       pair or size. Settlement is verified. Nothing is revealed.`
- ❌ `Our cutting-edge ZK technology revolutionizes the way you trade by
       seamlessly protecting your privacy across the entire stack.`

### 5.3 Labels

- Always uppercase, always tracked.
- Use bracket-wrapping for metadata (`[ TESTNET — LIVE ]`).
- Use slash-separated nouns for compound labels (`ORDERS/SEC`, `TRADES/TX`).
- Avoid Title Case. Avoid Sentence case. Avoid camelCase in user-visible
  labels.

### 5.4 Forbidden tone

| Don't                          | Why                                          |
| ------------------------------ | -------------------------------------------- |
| Exclamation marks              | DarkPool is calm, not enthusiastic           |
| Emoji in product copy          | Breaks the terminal aesthetic                |
| Marketing superlatives         | "Revolutionary," "next-gen," "best-in-class" |
| Second-person conversational   | No "you'll love it" or "we promise"          |
| Questions in headlines         | DarkPool states, never asks                  |
| Em-dashes inside sentences     | Reserve em-dashes for label separators (`[ A — B ]`) |

---

## 6. Accessibility

DarkPool aims for **WCAG 2.2 AA** as a baseline, AAA where it's free. The
list below covers known properties of the current visual language and
explicitly **flags gaps** that need to be closed before any production use.

### 6.1 Reduced motion

`prefers-reduced-motion: reduce` MUST disable:
- Mode B ambient loops (marquee, terminal-scroll, blink)
- Mode A entrance reveals (set final state directly)
- WebGL ambient canvases (replace with a static image or solid `brand.bg`)

**Status: gap.** The current landing source does not honor this media query.
Implementation contract is documented in §2.4.3 — adopt before shipping.

### 6.2 Color contrast

| Foreground       | Background | Ratio    | Verdict      |
| ---------------- | ---------- | -------- | ------------ |
| `white`          | `brand.bg` | 19.8 : 1 | AAA          |
| `brand.accent`   | `brand.bg` | 16.7 : 1 | AAA          |
| `brand.muted`    | `brand.bg` | 4.5 : 1  | AA (≥ 12 px) |
| `brand.border2`  | `brand.bg` | 1.8 : 1  | **Fails — decorative only** |
| `brand.bg`       | `brand.accent` | 16.7 : 1 | AAA (CTA text on lime fill) |

**Rule:** Never use `brand.border2` for text the user must read.

### 6.3 Cursor

`cursor: crosshair !important` is a brand decision (§1.5, §2.5.5). It has
**two known accessibility costs**:

1. Some assistive overlays (magnifiers, custom pointer styles) rely on the
   default cursor; forcing crosshair may break their expectations.
2. Crosshair signals "select a precise point" — users may misinterpret links
   as drawing targets.

Mitigation: scope the override to a `.brand-cursor` class on `<body>` that
the user can disable via a future preferences toggle. **Status: gap** — the
current implementation applies it globally with no opt-out.

### 6.4 Focus indicators

The current source has **no defined focus states**. Adopt this recipe for
every interactive element:

```css
:focus-visible {
  outline: 1px solid #D4FF00;
  outline-offset: 2px;
}
```

For elements that already use `brand.accent` as their fill (the primary
button), invert: a 1 px `white` outline at the same offset.

**Status: gap.** Add focus-visible styles to all components in §3 before any
production release.

### 6.5 Hit targets

| Component       | Current size                    | Verdict                  |
| --------------- | ------------------------------- | ------------------------ |
| Primary button  | `px-8 py-4` (~`136 × 48 px`)    | ✅ Above 44 × 44 min     |
| Ghost button    | same                            | ✅                       |
| Nav link        | `font-size: 11px` + parent padding | ⚠ **Tight** — wrap each `<a>` in a `py-2 px-2` zone to reach 32 × 32 minimum |
| Footer link     | same as nav                     | ⚠ Same — needs padding   |
| Status pill dot | `6 × 6 px` (display only)       | ✅ Not interactive       |

**Status: gap** for nav and footer links. Add internal padding to expand
the hit zone without changing visual density.

### 6.6 Screen-reader concerns

- The TerminalFeed is decorative ambient content. Mark its container
  `aria-hidden="true"`. (Not currently implemented — flagged.)
- The marquee Ticker repeats its text 4–8× to avoid visual gaps. Wrap the
  copies in `aria-hidden="true"` and expose **one** off-screen accessible
  copy with the same text.
- The dot-grid overlay (`body::before`) is a pseudo-element, so it's
  already invisible to assistive tech — no further action needed.

### 6.7 Internationalization

- Display copy uses `clamp()` for size — copy in longer languages
  (German, Portuguese) will scale down before overflowing.
- Mono labels with `tracking-[0.3em]` will wrap awkwardly in
  non-Latin scripts. For locales using CJK, JP, AR, or HE, drop tracking to
  `0` and switch the label family to a localized mono fallback.

---

## 7. Open Questions

These are intentionally unresolved. Decide before each affected surface
ships.

### 7.1 Does the dot-grid overlay survive into the in-app product?
The grid works as a hero atmospheric signal. In a dense trading UI (order
book, charts), the dot pattern at `z-9999` may interfere with reading data.
**Recommendation:** keep on marketing surfaces, disable inside the app shell.

### 7.2 Do we need a dim variant of the accent for charts?
A single `#D4FF00` works for a CTA. A candlestick chart needs at least two
data channels. Options:
- Add `brand.accent-2` at a desaturated lime (e.g. `#7A9000`)
- Use **opacity steps** of the same accent (40%, 70%, 100%) — preserves
  "one accent, never two" (§1.2)
- Use **fill vs stroke** as the second channel (filled = primary,
  outlined = secondary)
- Use **luminance** in monochrome — keep accent for the one "live" data
  point, and render the rest in `white → brand.muted` steps

**Recommendation:** the monochrome-with-accent route (last option) — keeps
the brand intact at the cost of needing a chart-typography spec.

### 7.3 Focus-state recipe
Defined in §6.4 above but **not yet implemented anywhere**. Decide once and
apply uniformly across §3 components.

### 7.4 Toast / inline notification component
Not designed yet. The §3.8 status pill is the closest existing primitive.
A toast would need a different ornament (probably a bracketed tag, see
§2.5.2) to avoid stealing the live-status semantic.

### 7.5 Data-table style
Trading UIs need dense tables. The mono type scale (§2.2.2) covers it, but
row separation, hover, and zebra striping have no spec. Open question:
single `border-b border-brand-border` per row vs. no separator with
generous `py-3` gaps.

---

## Appendix A — File references

The canonical implementations of every token, component, and pattern in
this document live at commit `a53f9fe` (`style(front): spread footer content
across center with developers column`). The working tree has since been
emptied; retrieve sources with:

```
git show a53f9fe:<path>
```

| Document section          | Source file (at `a53f9fe`)             |
| ------------------------- | -------------------------------------- |
| 2.1 Color                 | `front/tailwind.config.ts`             |
| 2.2 Typography            | `front/app/layout.tsx`, `tailwind.config.ts` |
| 2.3 Layout & Spacing      | All component files                    |
| 2.4 Motion (keyframes)    | `front/tailwind.config.ts`             |
| 2.4 Motion (GSAP)         | `front/components/Hero.tsx`, `HowItWorks.tsx` |
| 2.5.4 Dot-grid overlay    | `front/app/globals.css`                |
| 2.5.5 Crosshair cursor    | `front/app/globals.css`                |
| 3.1 / 3.2 Buttons         | `front/components/Hero.tsx`            |
| 3.3 Nav                   | `front/components/Nav.tsx`             |
| 3.4 Stat Block            | `front/components/Hero.tsx`            |
| 3.5 Step Card             | `front/components/HowItWorks.tsx`      |
| 3.6 Ticker                | `front/components/Ticker.tsx`          |
| 3.7 Terminal Feed         | `front/components/TerminalFeed.tsx`    |
| 3.8 Status Pill           | `front/components/Footer.tsx`          |
| 4.1 Hero pattern          | `front/components/Hero.tsx`            |
| 4.2 Section header        | `front/components/HowItWorks.tsx`      |
| 4.3 Timeline pattern      | `front/components/HowItWorks.tsx`      |
| 4.4 Stat bar pattern      | `front/components/HowItWorks.tsx`      |
| 4.5 Footer pattern        | `front/components/Footer.tsx`          |
