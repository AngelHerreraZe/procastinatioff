# Desktop design: Windows, macOS and Linux

Follows [DESIGN.md](../DESIGN.md): the two looks, tokens, components and rules. The three systems
share one interface (`app/ui`, Svelte): Tauri hosts it on Windows and Linux, and the macOS app will
host the same build in a `WKWebView`. This file covers the code, the shared layout, what changes per
system and every screen. Reference window: 1280 × 800 px (minimum width 360).

## 1. The theme in code

- `app/ui/src/app.css`: the tokens as CSS variables under `[data-theme='calma']` and
  `[data-theme='nocturno']`, plus the shared classes.
- `app/ui/src/App.svelte` computes `theme` with the shared rule and sets `data-theme` on `<html>`;
  no screen chooses a look.
- Components use `var(--token)` only, never a color value:

| Token | CSS variable |
|---|---|
| `bg`, `surface`, `chrome`, `text`, `muted` | `--bg`, `--surface`, `--chrome`, `--text`, `--muted` |
| `accent`, `on-accent`, `accent-soft` | `--accent`, `--on-accent`, `--accent-soft` |
| `track`, `border`, `danger` | `--track`, `--border`, `--danger` |
| Radius, weights, font | `--radius`, `--body-weight`, `--title-weight`, `--font` |

- Shared classes: `.btn-primary`, `.btn-secondary`, `.btn-tonal`, `.btn-text`, `.btn-danger`,
  `.card`, `.chip`, `.row`, `.stack`, `.muted`, `.num` (tabular figures), `.sr-only`, `.error`.
  A new pattern becomes a class here before a second screen copies it.
- The look change is the `body` color transition (300 ms); `prefers-reduced-motion` turns it off.
- Fonts: system fonts with each look's weights; never from the network (the CSP forbids it).

## 2. Shared layout

| Element | Calma | Nocturno |
|---|---|---|
| Side navigation | `chrome`, 232 to 260 px. Items 32 to 40 px; active item `surface` (Windows) or `border` (macOS, Linux) background with a 3 px `accent` bar (Windows) | `chrome`; active item `surface` with an amber bar; inactive items in `muted` |
| Header | Date 14/20 `muted` above the title 28/36; sync status on the right, 13 px `muted` | Title 30/38 800; on the right, the amber moon with "Sleep in 3 h 06 min" |
| Day status | Banner in `accent-soft`, radius 12, padding 18/20: lock in a 44 px `surface` circle, title 20/26, sentence, **primary action inside the banner** | `surface` card with `border`, radius 20, padding 22/24: title 26/32 800, sentence in `muted`, primary and secondary actions inside |
| Goals | List in a `border` box, radius 12; row padding 16/18; label chip; 8 px bar; per-row action (Continue, Start, View) as a small outline button | Grid of 3 `surface` cards, radius 18, padding 18 |
| Day summary | Right column (1 : 1.5 with the goals): Passes (with "Use a 5-min pass"), Tonight (bedtime and the 5-minute warning), Privacy ("The camera never records"); emergency as text at the end | Right column "Before bed": the 22:25 warning, what remains at 22:30 (calls only), privacy |
| Buttons | Primary 40 to 48 px; secondary outline in `accent` | Primary amber 44 to 48 px, weight 800; secondary `surface` + `border` |

- Navigation items are the same in both looks: Today, Goals, Passes, Devices, Privacy, Settings.
- Below 1000 px wide: one column. The sidebar becomes a row at the top with the same items (icon and
  label, never icons alone) and the summary moves under the goals. The page never scrolls sideways.
- Keyboard: Tab order follows the visual order (navigation, header, status and its action, goals,
  summary); Enter and Space activate; the focus ring is always visible.
- Closing the window never stops the protection, and the interface says so ("Closing this window
  does not stop the protection").

## 3. What changes per system

The window frame and its buttons are the system's own (native decorations): never redraw them.

| | Windows | macOS | Linux (GNOME) |
|---|---|---|---|
| Font fallback | Segoe UI Variable | SF Pro | Cantarell |
| Sidebar | 248 px (240 Nocturno), items 40 px, radius 6 | 232 px, items 32 to 34 px, radius 6 to 8; traffic lights above | 260 px with the app name centered at the top; items 40 px, radius 8 |
| Header | Inside the content: date + title, sync on the right | 56 px bar under the window controls: "Today · Thursday 13 November" + sync (Nocturno: the bedtime countdown) | 48 px header bar, title centered, menu button and a round close button on the right |
| Content | Main area on `surface` with a rounded top-left corner (8) against the sidebar; padding 28/36 | Two columns (Nocturno: content + 340 px column) | Centered column, 920 px max (Nocturno 820); boxed lists radius 12 (Nocturno 14) |
| Buttons | Pills | Radius 10 (Nocturno 12) | Radius 8 (Nocturno 10); secondary as `accent-soft` fill |
| Status actions | Primary inside the status banner | Primary inside the status card | Primary, secondary and emergency inside the status card |

## 4. Screens

| Screen (`app/ui/src/screens/`) | Pattern | Primary action |
|---|---|---|
| `Onboarding.svelte` | Create a profile or join one: device name, schedule, goals | Continue |
| `Home.svelte` | Today: sidebar, header, status, goals, summary | Continue the next goal |
| `PlanDay.svelte` | Today's plan, stricter only | Start the day |
| `Focus.svelte` | Focus layout: step, large timer, "Later", "No rush"; camera banner when verifying on the PC | Mark done or continue |
| `Shield.svelte` | Full screen over everything: why, next step, the grace countdown before closing apps; at bedtime only the moon and "calls keep working" | Open the next goal |
| `Pairing.svelte` | QR, then the 6-digit code shown large | Codes match |
| `Family.svelte` | Minor's PC: ask the guardian, what the guardian sees | Ask |
| `Settings.svelte` | Technical report; unlock (PIN, password or pattern); "Delete all my data" last, apart, with confirmation | Unlock |

## 5. Not done yet

- Inter and Manrope as local files (license review).
- The macOS host app (WKWebView): needs a Mac.
