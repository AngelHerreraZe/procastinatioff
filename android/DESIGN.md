# Android design

Follows [DESIGN.md](../DESIGN.md): the two looks, tokens, components and rules. This file covers how
they map to Compose, the sizes on Android and every screen. Reference size: 412 × 892 dp at 100 %
font scale.

## 1. The theme in code

- `app/src/main/java/dev/procastinatioff/app/ui/Common.kt`: `Look`, `lookFor(status, nowMs)` and
  `PcoTheme(look)`. `MainActivity` and `ShieldActivity` wrap everything in
  `PcoTheme(lookFor(snap.status, snap.nowMs))`; no screen picks a look by itself.
- Screens read `MaterialTheme.colorScheme`, never `Color(0x...)`:

| Token | Material role |
|---|---|
| `bg` | `background` |
| `surface` | `surface` |
| `text` | `onSurface`, `onBackground` |
| `muted` | `onSurfaceVariant` |
| `accent` | `primary` |
| `on-accent` | `onPrimary` |
| `accent-soft` | `primaryContainer`, `secondaryContainer` |
| `accent-pressed` | `onPrimaryContainer`, `onSecondaryContainer` |
| `track` | `surfaceVariant` |
| `chrome` | `surfaceContainerHigh` |
| `border` | `outlineVariant` |
| `muted` as input border | `outline` |
| `danger` | `error` |
| `success`, `warning`, `info` | `StatusColors` (no Material role; the same in both looks) |

- Every `surfaceContainer*` role is set, so no Material default (purple-gray) leaks into a component.
- Shapes: `extraSmall` = inputs (12 dp); `small` = chips and badges (pill); `medium` = cards (20 dp
  Calma, 24 dp Nocturno); `large` = 28 dp (dialogs).
- Type: Inter from `res/font/inter.ttf` (variable, OFL; license in `assets/licenses/`). Material 3
  roles map to the scale: `displaySmall` Display 32, `headlineMedium` H1 28, `headlineSmall` H2 24,
  `titleLarge` H3 20, `titleMedium` card titles 16, `bodyLarge` Body 16, `bodyMedium` Small 14,
  `titleSmall` steps and eyebrows 14 (in capitals), `labelLarge` buttons 16, `labelMedium` captions
  and badges 12, `displayLarge` the focus timer, `displayMedium` codes to compare (both tabular).
  `PcoTheme` raises the weights in Nocturno; do not set weights by look inside a screen.
- `PcoTheme` also sets the status and navigation bar icons by look (dark icons in Calma, light in
  Nocturno). Activities call `enableEdgeToEdge()`; `Screen` pads the safe areas and the keyboard.
- Icons: `PcoIcons` in `ui/Icons.kt`, one stroke set drawn from SVG paths. The logo is never redrawn:
  `Wordmark()` shows `R.drawable.logo`.
- **Camera verifier** (`verifier/`, a separate app without the core): the app passes the look as the
  `night` extra (`verifierIntent(..., night)`), and `VerifyActivity.kt` holds a copy of both color
  schemes. Change it together with `Common.kt`.

## 2. Layout

Every screen is a `Screen(...)` (`ui/Common.kt`): safe areas, an optional top row, a scrolling body
and the main actions pinned at the bottom. The components are in the same file: `AppCard`,
`Section`, `SectionHeader`, `IconCircle`, `IconText`, `Footnote`, `LinkRow`, `StatusBadge`,
`InfoChip`, `KindChip`, `ProgressBar`, `GoalRow`, `EmptyState`, `LoadingState`, `Wordmark`,
`PrimaryButton`, `SecondaryButton`, `IconAction`, `EmergencySection`, `ErrorText`. Reuse them; do
not restyle Material components inside a screen.

| Element | Calma | Nocturno |
|---|---|---|
| Top row | 56 dp: back arrow (48 dp) on the left; the step ("STEP 1 OF 2", 14 medium, `muted`) and 48 dp icon actions on the right. Icon actions show their label on long press | Same |
| Header | Eyebrow (date or "NOW", 14 medium capitals, `muted`) · title H1 28/36 bold · lead 16/24 in `text` | Title 800 |
| Content | Padding 8 top, 24 sides, 24 bottom; 16 between blocks; up to 640 dp wide, centered | Same |
| Cards | `surface`, 1 dp `border`, radius 20, padding 20 (16 for rows), no shadow | Radius 24 |
| Day status | 48 dp state icon on `accent-soft` (lock, ticket, clock, unlock, moon, alert); title H3; sentence in `muted`; chips 32 dp | Title 800 |
| Goals | One card per goal: name + badge, 8 dp bar, "12 of 30 min" + action | Same |
| Actions | Pinned at the bottom, in thumb reach; they scroll by themselves past 60 % of the height. Primary 56 dp pill · secondary 52 dp pill on `accent-soft` · emergency text with icon in `danger` | Primary amber, weight 800 · secondary on `accent-soft` |
| Sync footer | 13/18 `muted`, centered, check icon | Same |
| Bottom navigation (when added) | 80 dp, `surface`, top `border`; 4 items, icon 22 + label 12; active pill 64 × 32 `accent-soft` | 76 dp, `chrome`, top `border`; active item amber, weight 800 |

- Navigation items are the same in both looks: Today, Goals, Devices, Privacy. Settings stays in the
  top bar.
- Draw edge to edge and respect the system bars: the primary action always sits above the gesture bar.
- Tablets and landscape: content up to 640 dp wide, centered; long text never runs edge to edge.
- Ripple on every tappable element; disabled controls at reduced emphasis and not clickable.

## 3. Screens

Every screen below uses `PcoTheme` and the components in `ui/Common.kt` (`GoalRow`, `KindChip`,
`ProgressBar`, `PrimaryButton`, `SecondaryButton`, `EmergencySection`, `ErrorText`).

| Route | Screen (file) | Pattern | Primary action |
|---|---|---|---|
| `welcome` | `WelcomeScreen` (SetupScreens.kt) | Intro: what the app does, what it never does | Start |
| `verifier` | `VerifierScreen` (SetupScreens.kt) | "Step 1 of 3". Why there are two installs (camera without Internet vs. app with sync) and why now: once Accessibility is on, the blocker gets in the system installer's way. "Installed"/"Pending" badge; "Set up later" while missing | Install the verifier, then Continue |
| `permissions` | `PermissionsScreen` (SetupScreens.kt) | "Step 2 of 3". One card per permission: icon, name, "Granted"/"Pending" badge, why in one sentence, "Grant permission  >". "Set up later" while one is missing; from Today it is a checklist that returns | Continue (all three granted) |
| `choose` | `ChooseScreen` (SetupScreens.kt) | "Step 3 of 3". Four choice cards (icon, label, chevron): new, join, support someone, family | The chosen card |
| `config`, `family-new` | `ConfigScreen` (SetupScreens.kt) | Form: labels above fields, errors under them | Save |
| `join`, `watch-join` | `JoinScreen` (PairingScreens.kt) | Scan or paste the code, then compare the 6-digit code | Confirm the code matches |
| `invite`, `contact-invite`, `child/{id}/invite/{role}` | `InviteScreen` (PairingScreens.kt) | QR, then the 6-digit code shown large | Codes match |
| `home` | `HomeScreen` (DayScreens.kt) | Today: "Blocking is paused" card while Accessibility is off (links to `permissions`), day status, goals, actions | Continue the next goal |
| `plan` | `PlanScreen` (DayScreens.kt) | Today's plan, stricter only | Start the day |
| `focus/{goalId}` | `FocusScreen` (DayScreens.kt) | Focus layout: step, large timer, "Later", "No rush"; camera banner when verifying | Mark done or continue |
| `auth` | `AuthSettingsScreen` (AuthScreens.kt) | Unlock card first; then sections; report and "Delete all my data" last, apart | Unlock |
| `contact` | `ContactScreen` (ContactScreens.kt) | Trusted contact status and requests | Invite or answer |
| `watch` | `WatchScreen` (ContactScreens.kt) | The person I support: requests and alerts | Answer the request |
| `family` | `FamilyListScreen` (FamilyScreens.kt) | Minors' profiles as a list | Open a profile |
| `child/{id}` | `ChildScreen` (FamilyScreens.kt) | Mirror panel: what the minor sees, requests | Answer the request |
| `child/{id}/settings` | `ChildSettingsScreen` (FamilyScreens.kt) | Minor's rules | Save |
| `family-consent` | `FamilyConsentScreen` (FamilyScreens.kt) | Consent text and what the guardian will see | Accept |
| `tutor` | `TutorScreen` (FamilyScreens.kt) | Minor's view: ask the guardian | Ask |
| (activity) | `ShieldScreen` (shield/ShieldActivity.kt) | Shield: why, next step; at bedtime only the moon and "calls keep working" | Open the next goal |
| (verifier app) | `VerifyScreen` (verifier/.../VerifyActivity.kt) | Fixed "camera on" banner with Stop; instruction; progress | Stop or capture |

## 4. Not done yet

- Manrope for Nocturno (Inter is used in both looks for now).
- The look changes instantly; the 300 ms cross-fade is missing.
- There is no bottom navigation bar yet (the screens link to each other; Today's top row holds them).
- The camera verifier has the new colors but keeps the system font and its own layout.
