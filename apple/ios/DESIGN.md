# iOS design

Follows [DESIGN.md](../../DESIGN.md): the two looks, tokens, components and rules. This file covers
how they map to SwiftUI and UIKit, the sizes on iOS and every screen. Reference size: 390 × 844 pt
at the default Dynamic Type size.

## 1. The theme in code

- `Shared/Theme.swift`: `Palette` (the tokens) and `Theme.current(SharedState)`, with the same rule
  as Android and desktop. It reads the shared state, so the app and the Screen Time extensions use
  the same look.
- Colors only from the palette: `Color(uiColor: palette.accent)` in SwiftUI, `palette.accent` in
  UIKit. No hex values, no `.blue`, `.label` or `.systemBackground` in a screen.
- The app's root view computes the palette once (on each status refresh) and passes it down in the
  environment; screens never decide the look.
- Each look sets the color scheme: `.preferredColorScheme(palette.dark ? .dark : .light)`, so system
  controls (pickers, sheets, the keyboard) match.

## 2. Layout

| Element | Calma | Nocturno |
|---|---|---|
| Header | Date 13/18 semibold, uppercase, `muted`, above a large title 34/41 bold. Settings: 44 pt round button on `surface` | Date with the time; large title 800 |
| Content | 16 pt side margins; 20 pt between blocks | 18 pt between blocks |
| Day status | `surface`, radius 14, padding 16. Lock in a 44 pt tile (radius 12, `accent-soft`); title 20/25 semibold; next step 15/21 `muted`; "Bedtime 22:30 · 2 of 3 passes" 14/20 | `surface`, radius 20, padding 18; title 22/28 800; amber moon line "Bedtime 22:30 · in 3 h 06 min" |
| Goals | Inset grouped list (radius 14) under a 13/18 uppercase `muted` header. Row: name 17/22 semibold, label 13/18 `muted`, 6 pt bar, detail 15/20 | Same list on `surface`, radius 20; name weight 700 |
| Actions | At the bottom. Primary 52 pt, radius 14 · secondary 50 pt `accent-soft` fill with `accent` text · emergency 44 pt text in `danger` | Primary 54 pt amber, radius 16, weight 800 · secondary 50 pt `surface` fill · emergency `danger` text |
| Sync footer | 13/18 `muted`, centered | Same |
| Tab bar | `surface` with top `border`; 4 tabs, icon 26 + label 12; active in `accent` | `chrome` with top `border`; active amber, weight 800 |

- Tabs are the same in both looks: Today, Goals, Devices, Privacy. Settings opens from the header.
- Respect the safe areas (Dynamic Island, home indicator); the primary action never sits under the
  home indicator.
- Dynamic Type: use text styles (`.largeTitle`, `.headline`, `.body`, `.footnote`) with the weights
  above; layouts wrap instead of truncating.
- Haptics: a light impact when a goal is completed or a pass starts; nothing else.

## 3. Screen Time shields

`ShieldConfig/ShieldConfig.swift` already follows the look:

| Part | Value |
|---|---|
| Background | `bg` (solid, no blur) |
| Icon | SF Symbol in `accent`: `moon` (bedtime), `target` (focus), `lock` (locked) |
| Title / subtitle | `text` / `muted` |
| Primary button | "Got it": `accent` fill, `on-accent` text |
| Secondary button | "Open ProcastinaciOFF" in `accent`; none at bedtime (only calls) |

The shield cannot open the app; the secondary button posts a local notification that does.

## 4. Screens

No SwiftUI screens exist yet. They mirror Android ([android/DESIGN.md](../../android/DESIGN.md),
section 3) with iOS conventions:

| Screen | Notes for iOS |
|---|---|
| Welcome, Permissions | Screen Time authorization (`.individual`, or `.child` on a minor's iPhone), notifications, local network. One card per permission with why in one sentence |
| Choose, Configure | Form with grouped sections; app choice with `FamilyActivityPicker` |
| Pair (invite and join) | QR and the 6-digit code shown large; the camera for scanning shows "The camera never records" |
| Today | The layout in section 2 |
| Day plan, Focus | Focus: step, large timer (tabular figures), "Later", "No rush" |
| Settings | Unlock first (PIN, password, pattern, Face ID or Touch ID via `LocalAuth`); technical report; "Delete all my data" last and apart (`Erase.all()`) |
| Trusted contact, Family, Guardian, Minor | Same content as Android; lists in inset grouped style |
| Verifier flows | "Camera on" banner always visible with Stop; nothing is recorded |

## 5. Not done yet

- Every SwiftUI screen (needs a Mac with Xcode).
- Fonts: system font (SF Pro) with the weights of each look; Inter and Manrope come as local files.
