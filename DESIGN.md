# Design system

The rules every screen follows, on every platform. Each platform file adds sizes, navigation, where
the code lives and its list of screens:

- [Android](android/DESIGN.md) (app and camera verifier)
- [iOS](apple/ios/DESIGN.md) (app and Screen Time shields)
- [Desktop: Windows, macOS and Linux](desktop/DESIGN.md) (one shared interface)

A screen that breaks a rule here is a bug, like a failing test.

## 1. Two looks, chosen by the clock

| Look | When | Character |
|---|---|---|
| **Calma** | The rest of the day | Quiet and legible: deep teal on warm neutrals, plenty of space. It accompanies; it does not watch. |
| **Nocturno** | From 1 hour before bedtime, through SLEEP, until waking | Dark first and kind to sleep: navy and warm amber, low overall brightness, the bedtime countdown always visible. |

The rule is the same everywhere and comes from the core's status:

```text
Nocturno  if state == SLEEP
          or (sleep_at is known and now >= sleep_at - 60 min)
Calma     otherwise
```

- `sleep_at` is empty on a night before a free day (no SLEEP that night), so it stays Calma.
- Neither the user nor the system dark mode chooses the look, and there is no third look. The focus
  screen (a goal in progress) keeps its own layout, drawn in the current look.
- Everything follows the rule: app screens, blocking screens (shields), the camera verifier.
- Code: Android `lookFor()` in `android/app/.../ui/Common.kt` · desktop `theme` in
  `desktop/app/ui/src/App.svelte` · iOS `Theme.current()` in `apple/ios/Shared/Theme.swift`.
  Change all three together.
- The change is a 300 ms cross-fade of background and text colors; with reduced motion, instant.
- **Navigation does not change with the look.** The same destinations, in the same place, by day
  and by night. Nocturno adds the bedtime countdown to the status, not a new section.

## 2. Color

Screens use tokens, never a hex value.

| Token | Calma | Nocturno | Use |
|---|---|---|---|
| `bg` | `#F5F7F5` | `#0E1320` | Screen background |
| `surface` | `#FFFFFF` | `#161D2E` | Cards, lists, sheets |
| `chrome` | `#EEF1EF` | `#0B0F1A` | Desktop side navigation; phone navigation bar in Nocturno; subtle insets (neutral badges, report box) |
| `text` | `#172321` | `#ECEFF7` | Main text. Never pure white at night |
| `muted` | `#64736F` | `#A9B3C7` | Secondary text; input borders |
| `accent` | `#087C70` | `#F5B544` | Primary button, progress, links, icons, active navigation, the bedtime in Nocturno |
| `accent-pressed` | `#0E625B` | `#ECEFF7` | Text on `accent-soft` (labels, secondary buttons, the code to compare) |
| `on-accent` | `#FFFFFF` | `#1A1300` | Text on `accent` |
| `accent-soft` | `#DDF3ED` | `#2A3348` | Icon circles, labels, secondary buttons, selected chips |
| `track` | `#E6EBE8` | `#2A3348` | Progress bar background |
| `border` | `#E6EBE8` | `#2A3348` | Card borders and dividers (decorative) |
| `danger` | `#B3403C` | `#FCA5A5` | Emergency and destructive actions: as text, never as a fill (a confirmation step uses the normal primary button) |
| `success` · `on-success` | `#198754` · `#FFFFFF` | same | "Granted", "Done" badges |
| `warning` · `on-warning` | `#E9A23B` · `#172321` | same | "Pending", "N requests waiting" badges |
| `info` | `#4386C6` | same | Icons only (3.8:1 on white: never text) |

`danger` is the Error token `#D9534F` darkened in the same hue: `#D9534F` measures 3.96:1 on white,
below AA for text. Status colors are badge fills with their own text color, so they work on both looks.

Measured contrast (WCAG 2.2). Calma meets AA (4.5:1) for all text; Nocturno keeps text at AAA (7:1)
with one exception noted below.

| Pair | Calma | Nocturno |
|---|---|---|
| `text` on `bg` · `surface` · `chrome` | 15.02 · 16.17 · 14.22 | 16.12 · 14.61 · 16.63 |
| `muted` on `bg` · `surface` | 4.62 · 4.97 | 8.79 · 7.97 |
| `muted` on `chrome` | 4.37: **do not use**; put `text` on `chrome` | 9.08 |
| `accent` as text on `bg` · `surface` | 4.73 · 5.09 | 10.22 · 9.27 |
| `on-accent` on `accent` | 5.09 | 10.18 |
| `text` on `accent-soft` | 13.95 | 10.96 |
| `accent-pressed` on `accent-soft` | 6.21 | 10.96 |
| `accent` on `accent-soft` | 4.39: icons only (3:1) | 6.95 |
| `muted` on `accent-soft` | 4.29: **do not use** | 5.98: **do not use** |
| `danger` on `bg` · `surface` | 5.25 · 5.65 | 9.77 · 8.86 |
| `on-success` on `success` · `on-warning` on `warning` | 4.53 · 7.47 | same |
| `accent` bar on `track` (non-text, 3:1) | 4.22 | 6.95 |
| `muted` input border on `surface` (non-text, 3:1) | 4.97 | 7.97 |

- Borders (1.2:1) are decoration. A control is never identified by its border alone: it has a label,
  a fill or an `accent` outline. Input borders use `muted`, which passes 3:1.
- Focus ring: 3 px solid `accent`, offset 2 px (Calma) or 3 px (Nocturno).
- Amber can read as "warning"; that is why emergency uses its own color and always says
  "Emergency".

## 3. Type

| | Calma | Nocturno |
|---|---|---|
| Family | Inter | Inter (Manrope later, once its file is reviewed) |
| Body | 400 | 500 (light text on dark looks thinner) |
| Titles | 600 to 700 | 800 |

Inter (SIL OFL 1.1) ships as a local file and is never loaded from the network: Android bundles the
variable font (`res/font/inter.ttf`, license in `assets/licenses/Inter-OFL.txt`). iOS and desktop
still use their system font (SF Pro, Segoe UI Variable, Cantarell) with these weights.

Scale at 100 % (phone: Display 32 · H1 28 · H2 24 · H3 20 · Body 16 · Small 14 · Caption 12 ·
Buttons 16); text grows with the system setting and nothing may be cut off:

| Role | Android | iOS | Desktop |
|---|---|---|---|
| Welcome title (Display) | 32/40 bold | 34/41 | 32/40 |
| Screen title (H1) | 28/36 bold | 34/41 (large title) | 28/36 (Nocturno 30/38) |
| Day status, section header (H3) | 20/28 semibold (Nocturno 800) | 20/25 (Nocturno 22/28) | 20-22/28 (Nocturno 26-28/32-34) |
| Goal name, card title | 16/24 semibold | 17/22 | 15-16/20-22 |
| Body | 16/24 (Nocturno 500) | 15-16/21-23 | 15/22 (Nocturno 16/24) |
| Secondary (Small) | 14/20 | 15/20 | 14/20 |
| Step, eyebrow | 14/20 medium, capitals | 13/18 | 13/18 |
| Caption, badge | 12/16 medium or semibold | 13/18 | 12-13/16-18 |

- Nothing below 12.
- Counters, times and minutes use tabular figures.

## 4. Space, shape, depth

- 4-point grid (4, 8, 12, 16, 20, 24, 32, 40, 48). 16 between blocks, 8 to 12 inside a block.
  Side margins: 24 on phones, 28 to 40 on desktop.
- No shadows. Calma separates with background color and a 1 px `border`. Nocturno separates by
  brightness: `bg` < `surface` < `surface` with `border`.
- Radii (wider at night, it softens the dark):

| | Calma | Nocturno |
|---|---|---|
| Phone cards | 20 (Android) · 14 (iOS grouped) | 24 (Android) · 20 (iOS) |
| Desktop cards | 12 | 20 status · 14-18 lists and cards |
| Buttons | pill (Android, Windows) · 14 (iOS) · 10 (macOS) · 8 (Linux) | pill (Android, Windows) · 16 (iOS) · 12 (macOS) · 10 (Linux) |
| Inputs | 12 (Android) | 12 (Android) |
| Chips, badges | pill | pill |

- Touch targets: 48 dp on Android, 44 pt on iOS, 32 px minimum on desktop (36 to 48 in practice),
  with 8 between targets. Phone buttons are at least 52 dp tall (primary 56).

## 5. Components

The anatomy is shared; the platform files give sizes.

| Component | Calma | Nocturno |
|---|---|---|
| **Day status** (first thing on Today) | State icon in a 48 circle of `accent-soft`; title ("Locked"); one sentence with the next step; chips for bedtime and passes | Title in 800; amber moon with "Sleep in 3 h 06 min"; passes in the sentence |
| **Goal row** | Name (600) · badge "Unlocks", "Habit" or "Done" · 8 px bar (`accent` on `track`) · detail in `muted` · the action at the end. Phones: one card per goal | Same content. Desktop: grid of 3 cards |
| **Primary button** (one per screen) | `accent` fill, `on-accent` text 600. Carries the key fact ("Continue reading · 18 min left"). Ignores repeated taps while working; a spinner appears after 300 ms | Amber fill, near-black text 800 |
| **Secondary button** | `accent-soft` fill with `accent-pressed` text (Android, iOS, Linux) or `accent` outline (Windows, macOS). Destructive variant: `danger` outline and text | `accent-soft` fill with `text` |
| **Text action** | `accent` text; with a chevron when it opens something ("Grant permission  >") | Amber text |
| **Emergency** | Text button in `danger` with a warning icon; never filled, never competing with the primary | `danger` text, no fill |
| **Feature / permission row** | 48 to 56 circle of `accent-soft` with the `accent` icon; title 600; one sentence in `muted`; status badge on the right | Same |
| **Status badge** | Pill, 12 semibold: `success` "Granted"/"Done", `warning` "Pending", `accent-soft` "Unlocks", `chrome` "Habit". Always a word, never color alone | Same |
| **Chip** | `bg` fill with `border`, height 32, icon 16 | `bg` fill with `border` |
| **Footnote** | Info (or topic) icon + one line in `muted`, for privacy and safety lines ("Calls and emergencies stay available") | Same |
| **Progress** | Value also written ("12 of 30 min"); `progressbar` role with its value | Same |
| **Sync footer** | Check icon + "Synced with «Study PC» 1 min ago", 13/18 `muted` | Same |
| **Navigation** | Active item: `accent-soft` pill (Android), `accent` icon and label (iOS), 3 px `accent` bar (desktop) | Active item in amber, weight 800 |
| **Shield** (blocking screen) | Full screen in the current look: why it is blocked, the next step, one primary action | At bedtime: moon, "Bedtime · calls keep working", no other action |
| **Focus screen** | "Step N of M", a large timer, "Later" and "No rush"; one task on screen | Same layout in Nocturno colors |

Icons: one line-icon set (24 grid, 2 px stroke, round caps and joins), decorative ones hidden from
screen readers; never emojis. Android draws them from SVG paths in `ui/Icons.kt` (no icon library).

Logo: `branding/logo.png` is the only logo, used everywhere the brand appears (app icons, launch
and welcome screens, notifications, installer, README). Never redraw or recolor it: replace the
file and run `python scripts/make_icons.py` to regenerate every derived asset.

## 6. States

| State | Says | Shows | Look |
|---|---|---|---|
| SLEEP | "Bedtime · calls keep working" | Moon; dimmed screen | Always Nocturno |
| LOCKED | "Locked · N goals left" | Lock; primary action = next goal | By the clock |
| FOCUS | "Focus: <goal>" + timer | Large timer; only the task's apps | By the clock |
| PASS | "Pass active · M min left" | Visible countdown | By the clock |
| FREE | "Free until 22:30" | No shields; quiet summary | By the clock |
| EMERGENCY | "Emergency · 15 min · it is recorded" | `danger` color and text | By the clock |

## 7. Rules for every screen

1. **One primary action**, the one that gets the person closest to unlocking.
2. **Always explain the block and the next step**, never only "Locked".
3. **Progress in numbers and visually** for every goal.
4. **No guilt.** Neutral, kind tone; no red for "you failed"; no fake countdowns or shaming.
5. **At most 3 actions** in the main view; the rest in menus or "more".
6. **Big targets where the hand is**: bottom of the screen on phones, beside the status on desktop.
7. **Recognize, don't recall**: goals and passes left are on screen.
8. **Friction only where it protects**: emergency (wait, reason, confirm) and changes that relax
   the rules. Nothing else gets extra steps.
9. **System state always visible**: sync, camera on, protection active, bedtime, in plain words.
10. **Privacy at the moment of use**: "The camera never records" next to the camera; "Your guardian
    sees the same as you" next to guardian features.
11. **Warn before taking something away**: 5 minutes before bedtime and 60 seconds before closing
    apps, with a visible countdown that screen readers announce.
12. **Platform conventions**: bottom navigation on phones, sidebar on desktop, native window
    controls and system gestures.
13. **Destructive actions apart**: "Delete all my data" and similar sit at the end of their screen,
    separated, with a confirmation step.

## 8. Words

- Speak to the person directly and kindly ("you have 2 goals left"); sentence case.
- Numbers with units: "12 of 30 min", "2 of 3 passes", "22:30".
- Fixed labels: **Unlocks** (key goal) and **Habit** (goal that does not block).
- Buttons start with a verb: "Continue reading", "Use a 5-min pass", "Ask Laura for access".
- Short, concrete privacy lines: "The camera never records".
- Every visible text lives in the translation files (English source, local translations); the core
  sends codes, never sentences.
- Never: "You failed!", "You lost your streak", repeated exclamation marks, emojis as information.

## 9. Accessibility (WCAG 2.2 AA at least)

- Contrast as measured in section 2; never color alone: state and goal type are always written.
  Blue/orange pairs rather than red/green.
- Real buttons, links and inputs with labels; headings per section; progress bars with their value;
  decorative icons hidden.
- Text scaling: Dynamic Type (iOS), font scale (Android), 200 % zoom (desktop) without losing content.
- Keyboard on desktop: everything reachable with Tab, Shift+Tab, Enter and Space, in visual order;
  the focus ring always visible.
- Countdowns (pass, grace, bedtime) are announced and explained; grace is at least 60 seconds.
- Plain language: sentences of 15 words or fewer.
- Test with TalkBack, VoiceOver, Narrator and Orca, and with Accessibility Scanner, Accessibility
  Inspector, Accessibility Insights and axe.

## 10. Motion

- Calma: 150 to 200 ms (opacity and height); progress bars grow when they change.
- Nocturno: minimal; the look change and entering SLEEP fade in 300 ms.
- Only transform and opacity; nothing blinks; everything turns off with reduced motion.

## 11. Before merging a screen

- [ ] Checked in **both looks** (a test profile with bedtime in the next hour shows Nocturno).
- [ ] Only tokens; no hex values or platform default colors in the screen.
- [ ] One primary action; at most 3 visible actions; emergency is text in `danger`.
- [ ] Every text pair is in section 2's table (or measured and added to it).
- [ ] Targets at the platform minimum; focus visible; screen reader order equals visual order.
- [ ] Works at the largest text size and with reduced motion.
- [ ] All texts in the translation files, English and Spanish.
