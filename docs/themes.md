# Themes

Takt's look is a theme: a colour palette and a set of structural values
(radii, borders, spacing, type) under a name. Three ship with every app — the
Mac, the iPhone and Android:

- **Takt**, the default. Friendly and roomy, the way a modern app is: Inter
  with Geist Mono for numerals, 12px cards and 8px rows and controls, and a
  more generous spacing scale, in the house colours: chalk paper (`#faf8f4`),
  grape ink (`#444054`) and an azure accent (`#007fff`), with dark the same
  grape pulled down (`#1c1a23`). It is grown from seeds, so it is the easiest
  one to make your own. Its identifier is still `native.theme.priority`, its old
  name, because that is what people's saved choice says.
- **Zed**, the Zed look: IBM Plex Sans and Lilex, square panels, hairlines,
  warm paper and grape ink. It was called Chalk, and its identifier still is
  (`native.theme.chalk`).
- **Zed Dark**, Zed with the appearance fixed to dark.
- **Grape**, the house design language whole: Zed's paper and grape ink, set in
  Arvo with Geist Mono for numerals and code, 8px panels and 6px controls, and
  10pt bold tracked capitals for every label (`native.theme.grape`).

You can add your own the way Zed does: as JSON files in a folder.

```
~/Library/Application Support/Takt/themes/*.json
```

Every `.json` file there is a theme. It appears in **Settings → Appearance** beside
the built-ins, and the app reloads it as you save it. If the theme you are
using is edited, the window redraws; if its file disappears or stops loading,
the default (Takt) stands in until it comes back. Your themes and your choice sync to
the iPhone and Android.

## Getting started

The quickest way in is from the command palette (`⌘K`):

- **Export the current theme as JSON** writes the theme you are using to the
  folder as a complete file — every colour and size spelled out — opens it in
  your editor, and switches to it. Whatever you change in it shows up as soon
  as you save.
- **Open the themes folder** reveals the folder in Finder, creating it if it
  is missing.
- **Reload themes** re-reads the folder. It reloads by itself when a file is
  saved; this is for when it did not.

The same three are buttons under **Your themes** in the theme settings page,
which also lists every problem found in the files.

## A theme can be three colours

Every field is optional. Anything a file leaves out comes from the theme it
**extends**, which is the default, Takt (`native.theme.priority`), unless it says otherwise.

The quickest start is **seeds**: the page, the text and the accent, for each
appearance. The app works out the other neutrals (rows, hover, wells, the
three border strengths, muted and dim text) by mixing the text into the page
in fixed steps, so they always sit in the right order and keep their contrast.
This is a complete theme:

```json
{
  "name": "Sea",
  "seeds": {
    "light": { "background": "#f4f8f9", "foreground": "#14303a", "accent": "#0b7a8c" },
    "dark":  { "background": "#0e1d22", "foreground": "#dcecef", "accent": "#4fc3d4" }
  }
}
```

| Seed | Becomes |
| --- | --- |
| `background` | `paper`, and the base of every neutral |
| `foreground` | `ink`, and the other end of every neutral |
| `accent` | `primary` |
| `success`, `danger`, `warning` | the status colours of the same name |

How the neutrals are mixed, as a share of the way from background to
foreground: `altRow` 2.5%, `hover` 5%, `well` and `borderMuted` 7%, `border`
12%, `inputBorder` 20%, `dimText` 38%, `mutedText` 75%. `raised`, for cards,
is the one that sits *above* the page: 60% of the way to white in light, and
4% towards the text in dark. Takt itself is made this way, naming only its
`raised` so the dark card stands a clear step above the page.

A seed you leave out comes from the theme you extend, so a file can give only
an accent. If one colour comes out wrong, name it under `palette` — anything
in `palette` wins over what the seeds produced:

```json
{
  "name": "Sea, tuned",
  "seeds": { "light": { "background": "#f4f8f9", "foreground": "#14303a", "accent": "#0b7a8c" } },
  "palette": { "light": { "border": "#c9d9dd" } }
}
```

You can also skip seeds and change single roles. This is Takt with a
violet accent:

```json
{
  "name": "Takt, violet",
  "palette": {
    "light": { "primary": "#7a4de8" },
    "dark": { "primary": "#9b7bf0" }
  }
}
```

Or start from Zed instead, with bigger type and rounder corners:

```json
{
  "name": "Big Zed Dark",
  "extends": "native.theme.chalk.dark",
  "structure": {
    "radius": { "panel": 8, "row": 6, "control": 6 },
    "typography": { "bodySize": 15 }
  }
}
```

## The full format

This is what **Export** writes for Zed, reordered for reading. Every value
is stated, so it is also a reference for what can be set.

```json
{
  "identifier": "user.chalk-copy",
  "name": "Chalk copy",
  "summary": "Exported from Chalk. Edit freely; see docs/themes.md.",
  "lockedAppearance": null,
  "palette": {
    "light": {
      "paper": "#faf8f4",
      "raised": "#ffffff",
      "altRow": "#f5f3f1",
      "hover": "#f1eff1",
      "well": "#edebef",
      "border": "#e6e4ea",
      "borderMuted": "#efedf2",
      "inputBorder": "#d8d5dd",
      "ink": "#444054",
      "mutedText": "#6e6b7c",
      "dimText": "#b6b3bf",
      "primary": "#007fff",
      "success": "#4cc38e",
      "danger": "#d62246",
      "warning": "#ffbf00",
      "categoricalPurple": "#7a4de8",
      "categoricalPink": "#ff88dc",
      "categoricalOrange": "#ff6b2b",
      "mediaLetterbox": "#000000",
      "mediaScrim": "#000000b3",
      "mediaScrimInk": "#ffffff"
    },
    "dark": {
      "paper": "#1c1a23",
      "raised": "#25232f",
      "altRow": "#211f29",
      "hover": "#2d2b38",
      "well": "#2d2b38",
      "border": "#34313f",
      "borderMuted": "#2d2b38",
      "inputBorder": "#403d4d",
      "ink": "#f5f4f7",
      "mutedText": "#b6b3bf",
      "dimText": "#6e6b7c",
      "primary": "#007fff",
      "success": "#4cc38e",
      "danger": "#d62246",
      "warning": "#ffbf00",
      "categoricalPurple": "#7a4de8",
      "categoricalPink": "#ff88dc",
      "categoricalOrange": "#ff6b2b"
    }
  },
  "structure": {
    "radius": { "panel": 0, "row": 0, "control": 4, "pill": 9999, "shell": 20 },
    "border": { "hairline": 1, "emphasis": 2, "focusRing": 2 },
    "spacing": { "xxs": 2, "xs": 4, "sm": 8, "md": 12, "lg": 16, "xl": 24 },
    "typography": {
      "display": { "families": ["IBM Plex Sans"], "design": "sans" },
      "body": { "families": ["IBM Plex Sans"], "design": "sans" },
      "mono": { "families": ["Lilex"], "design": "monospaced" },
      "bodySize": 13,
      "scale": { "caption": 12, "body": 13, "title": 15, "display": 28, "hero": 64 },
      "microLabel": {
        "size": 12,
        "weight": "regular",
        "tracking": 0,
        "uppercase": false,
        "role": "mutedText"
      }
    },
    "usesShadows": false,
    "usesGradientsOnChrome": false
  }
}
```

### Identity

| Key | Meaning |
| --- | --- |
| `identifier` | What the app stores as your choice. Optional: a file without one is `user.<file name>`, so `dusk.json` is `user.dusk`. It cannot be a built-in's (`native.theme.priority`, `native.theme.chalk`, `native.theme.chalk.dark`), and two files cannot share one — the second, in file-name order, is skipped. |
| `name` | Shown in the picker. Defaults to the file name. Not inherited. |
| `summary` | Shown under the picker. Not inherited. |
| `extends` | The identifier of the theme to take every unstated value from: a built-in, or another of your files (in any order; a cycle is reported and both are skipped). Leave it out for Takt, the default; `"native.theme.chalk"` is Zed. `null` inherits no colours at all — see [Missing roles](#missing-roles). |
| `lockedAppearance` | `"light"` or `"dark"` makes the theme *be* that appearance whatever the system is set to, the way Zed Dark does. `null` clears an inherited lock, so a theme extending Zed Dark can follow the system again. Leave it out to inherit. |

### Palette

`palette.light` and `palette.dark` map a **role** to a colour. Components only
ever name a role, never a hex value, so changing a role changes everything
drawn in it. A role you leave out keeps the value of the theme you extend, in
that appearance.

Colours are `#rgb`, `#rrggbb` or `#rrggbbaa` (the last two digits are alpha:
`#000000b3` is black at 70%).

| Role | Used for |
| --- | --- |
| `paper` | The page. |
| `raised` | Cards and panels, half a step above the page. |
| `altRow` | Alternating rows. |
| `hover` | Hover feedback on a row or control. |
| `well` | Wells, chips and inset containers. |
| `border` | The hairline that does the separating. |
| `borderMuted` | A softer hairline, for dividers inside a surface. |
| `inputBorder` | The stronger hairline an editable field needs. |
| `ink` | Body text and icons. |
| `mutedText` | Captions, micro-labels, status lines. |
| `dimText` | Placeholders and disabled glyphs — never anything you must read. |
| `primary` | Actions, links, focus rings, selection, "info". |
| `success`, `danger`, `warning` | The rest of the four-way status convention. |
| `categoricalPurple`, `categoricalPink`, `categoricalOrange` | Identity colour for charts, avatars and per-item hues. Never chrome. |
| `mediaLetterbox`, `mediaScrim`, `mediaScrimInk` | Behind and over photos. These do not flip with the appearance: they are read from `light` only, whatever is in force. |

Selection fills and status tints are made from these at low alpha where they
are drawn, so a new `primary` gives you a new selection colour too.

### Structure

All sizes are in points. Each group can be given in part.

| Key | Meaning |
| --- | --- |
| `radius.panel` | Cards, panels, popovers and overlays. Zed sets 0, the way Zed draws them; Takt sets 12. |
| `radius.row` | A list row's selection and hover — sidebar, outline, Today, result lists. Rows run edge to edge of their pane, so the built-ins set 0 and a selection is a full-width band; raise it for inset, rounded selections. |
| `radius.control` | Buttons, inputs, chips, tooltips. Should not exceed `panel`, unless `panel` is 0 — square panels with slightly rounded buttons is the built-in look. |
| `radius.pill` | Genuinely round things: avatars, dots, toggle knobs. Keep it large (999 or more). |
| `radius.shell` | The outermost window shell only. 18–22, or 0 for none. |
| `border.hairline` | The default rule. Above 2 is reported: that is a border, not a hairline. |
| `border.emphasis` | A selected or active edge. |
| `border.focusRing` | The focus ring, drawn in `primary`. |
| `spacing.xxs` … `spacing.xl` | The six-step spacing scale every padding comes from. Shrink them all for a denser app. |
| `typography.display`, `.body`, `.mono` | A face: `families`, tried in order, and `design` — `serif`, `sans`, `monospaced` or `rounded` — used when none of the families is installed. A family is a request: Takt bundles IBM Plex Sans and Lilex (the faces Zed ships), Inter, Geist, Geist Mono, JetBrains Mono and Arvo on every platform, and anything else has to be installed on the device. Each weight uses the family's own face where it has one. |
| `typography.bodySize` | The base text size. Changing it without giving a `scale` re-proportions the whole scale from it, so one number makes everything bigger. |
| `typography.scale` | The named sizes views ask for: `caption`, `body`, `title`, `display` (large numerals) and `hero` (the focus timer). Any you state win over the proportioned ones. |
| `typography.microLabel` | The small label on section headers, column heads, tabs and chips: `size`, `weight` (`regular`, `medium`, `semibold`, `bold`, `black`), `tracking` in em, `uppercase`, and the colour `role` it is set in. Zed sets it at caption size, regular, untracked, as written; Takt at 11pt semibold, tracked 0.02em; Grape as 10pt bold tracked capitals. |
| `usesShadows`, `usesGradientsOnChrome` | Declarations for the audit. Nothing in the app draws either; setting one true is reported. |

### The old slab style

Zed (as Chalk) used to be set in Arvo, with 10pt bold capitals tracked at 0.15em for
its labels. It is Zed's IBM Plex Sans and Lilex now, with quiet sentence-case
labels, but the old look is a theme away:

```json
{
  "name": "Chalk, slab",
  "structure": {
    "typography": {
      "display": { "families": ["Arvo", "Rockwell"], "design": "serif" },
      "body": { "families": ["Arvo", "Rockwell"], "design": "serif" },
      "scale": { "caption": 11 },
      "microLabel": { "size": 10, "weight": "bold", "tracking": 0.15, "uppercase": true }
    }
  }
}
```

Arvo is bundled, so this looks the same on every device; Rockwell is only the
fallback for a family that fails to load. The labels' text is written in sentence case and uppercased by the
theme, so it comes out as it did.

## Zed themes

A Zed theme file can go straight into the themes folder. Zed's format is a
*family*:

```json
{
  "$schema": "https://zed.dev/schema/themes/v0.2.0.json",
  "name": "Meadow",
  "author": "Takt",
  "themes": [
    { "name": "Meadow Dusk", "appearance": "dark", "style": { "editor.background": "#161c1aff", "text": "#dfe8e2ff", "…": "…" } },
    { "name": "Meadow Morning", "appearance": "light", "style": { "…": "…" } }
  ]
}
```

A file with a `themes` list, or a `$schema` on zed.dev, is read as one. Each
entry in `themes` becomes a theme of its own (the whole of this example is
[`shared/themes/zed/meadow.json`](../shared/themes/zed/meadow.json)):

| | |
| --- | --- |
| Identifier | `zed.` and the variant's name, lowercased and hyphenated: `zed.meadow-dusk`. Zed keys a theme by its name, and so does this, so the same file gives the same identifiers on every device. Two variants with the same name are one identifier, and the second is skipped. |
| Name | The variant's `name`. The summary names the family and its `author`. |
| `lockedAppearance` | The variant's `appearance`. A variant without one, or with something other than `light` or `dark`, is guessed from its page and warned about. |
| `extends` | `native.theme.one`, Zed One: so the structure is Zed's — IBM Plex Sans and Lilex, square panels, hairlines, and the phones' sizes — and any role the mapping cannot fill is One Light's or One Dark's. |
| Palette | Every role, the same in `light` and `dark`, from the table below. |

How a variant's `style` maps onto the roles — the first key it states wins,
and it is the mapping the Mac app uses to draw Zed's own One Dark and One
Light:

| Role | Zed keys, in order |
| --- | --- |
| `paper` | `editor.background`, `background` |
| `raised` | `elevated_surface.background`, `surface.background`, `panel.background` |
| `altRow` | `panel.background`, `surface.background` |
| `hover` | `element.hover`, `ghost_element.hover` |
| `well` | `background`, `element.background` |
| `border` | `border` |
| `borderMuted` | `border.variant`, `border` |
| `inputBorder` | `border` |
| `ink` | `text`, `editor.foreground` |
| `mutedText` | `text.muted` |
| `dimText` | `text.placeholder`, `text.disabled` |
| `primary` | the first of `text.accent`, `icon.accent`, `border.focused`, `players[0].cursor`, `accents[0]`, `info`, `link_text.hover`, `terminal.ansi.blue`, then any colour below, that stands 3:1 off the page (it carries the focus ring); failing all, the one that stands furthest off it |
| `success` | `success`, `created` |
| `danger` | `error`, `deleted` |
| `warning` | `warning`, `modified` |
| `categoricalPurple`, `categoricalPink`, `categoricalOrange` | the colour nearest violet (265°), pink (320°) and orange (25°), within 30° and not grey, from `accents`, each of `players`' `cursor`, the `terminal.ansi.*` colours (not the dim ones) and `syntax.*.color`, in that order |
| `mediaLetterbox`, `mediaScrim`, `mediaScrimInk` | not mapped: Zed One's |

A role none of its keys gives is grown from the variant's page and text, the
way [seeds](#a-theme-can-be-three-colours) grow one, so a sparse variant still
looks like itself rather than like One. Zed's colours may be translucent
(`#ffffff14`, and `#rgba` is read as well as `#rgb`, `#rrggbb` and
`#rrggbbaa`): the page is laid over `background` (or white or black for the
appearance), and every other colour over the page, so each role is opaque.
Takt has no role for `hint`, `players`' selections, `syntax` as such, the
scrollbars, tabs, title bar or terminal, so those are only read for the
identity colours above. A value that is not a colour is a warning and skipped;
the rest of the variant still loads.

Zed themes go through the same loader as every other file, so the audit runs
on them, they sync as `themes` rows (one row per file, under its first
variant's identifier), and the phones' **Import** takes them. A theme can
`extends` a Zed variant by its identifier from another file in the same folder.

### Zed's own themes folder, on the Mac

Zed reads `~/.config/zed/themes/*.json` (`$XDG_CONFIG_HOME/zed/themes` where
that is set). The Mac app reads that folder as well, read-only, so the themes
you have in Zed are in Takt's picker too, marked "Zed theme". Nothing there is
written, synced or reported beyond what stops a file loading; a file in Takt's
own folder with the same identifier wins. To take one of them to your phone,
copy the file into Takt's folder. `TAKT_ZED_THEMES` points the app at another
folder, or, set empty, at none.

## Across the Mac, iPhone and Android

One theme file works on all three apps. The palette is the same everywhere:
a role is one colour whatever the device, so a theme you tuned on the Mac
looks like itself on the phone. Structure (sizes, radii, spacing) is not the
same everywhere, because 13pt body text is right at a desk and too small in the
hand. So a theme can say what changes per platform.

### `platforms`

```json
{
  "name": "Dusk",
  "palette": { "dark": { "paper": "#15131c" } },
  "structure": { "radius": { "control": 2 } },
  "platforms": {
    "ios": { "structure": { "typography": { "bodySize": 18 } } },
    "android": { "structure": { "spacing": { "md": 18 } } }
  }
}
```

`platforms.macos`, `platforms.ios` and `platforms.android` each hold a partial
`structure`, laid over the theme's own `structure` on that platform only. The
order a value is resolved in, latest winning:

1. The theme it extends, fully resolved **for this platform** (which includes
   that theme's own `platforms` block).
2. This theme's `structure`.
3. This theme's `platforms.<this platform>.structure`.

So a theme that only changes colours inherits each platform's sensible sizes
from Takt (or from Zed, if it extends Zed), and a theme that sets `structure.typography.bodySize` sets it
everywhere unless a `platforms` entry says otherwise. Palette is not allowed
under `platforms`. A `palette` key there is a warning and is ignored, because
the point is that colour stays consistent.

### The built-ins per platform

Takt (`native.theme.priority`):

| | macOS | iOS | Android |
| --- | --- | --- | --- |
| `typography.bodySize` | 13 | 17 | 16 |
| `typography.scale` caption / body / title / display / hero | 12 / 13 / 16 / 30 / 64 | 13 / 17 / 20 / 34 / 72 | 12 / 16 / 20 / 32 / 72 |
| `radius` panel / row / control / shell | 12 / 8 / 8 / 18 | 14 / 10 / 10 / 18 | 16 / 12 / 10 / 18 |
| `spacing` xxs … xl | 2 6 10 14 20 28 | 2 6 10 14 20 28 | 2 6 10 14 20 28 |
| `touchTarget` | 0 (pointer) | 44 | 48 |
| fonts | Inter, Geist Mono | the same | the same |

Zed:

| | macOS | iOS | Android |
| --- | --- | --- | --- |
| `typography.bodySize` | 13 | 17 | 16 |
| `typography.scale` caption / body / title / display / hero | 12 / 13 / 15 / 28 / 64 | 13 / 17 / 20 / 34 / 72 | 12 / 16 / 20 / 32 / 72 |
| `typography.microLabel.size` | 12 | 13 | 12 |
| `radius` panel / row / control | 0 / 0 / 4 | 8 / 0 / 6 | 8 / 0 / 6 |
| `spacing` xxs … xl | 2 4 8 12 16 24 | 2 4 8 12 16 24 | 2 4 8 12 16 24 |
| `touchTarget` | 0 (pointer) | 44 | 48 |
| fonts | IBM Plex Sans, Lilex | the same | the same |

Zed Dark extends Zed and so takes the same per-platform structure.

`touchTarget` is new in the structure: the minimum hit area a control is
grown to, invisibly, so the painted control keeps its size. 0 means a pointer
platform with no minimum.

### Built-ins are shared files

The built-ins are defined once, in the Rust core
(`core/src/theme/builtins.rs`), which the Mac, the iPhone and Android all
take them from, and exported as complete JSON to
`shared/themes/priority.json`, `shared/themes/chalk.json` (Zed),
`shared/themes/chalk-dark.json` (Zed Dark), `shared/themes/grape.json` and
`shared/themes/one.json` (Zed One: Zed's own One Light and One Dark, the Rust
Mac app's look, which it writes to `theme.selected` once and on every change
so the phones follow it).
The core's tests fail if those files and its definitions disagree; run them
with `TAKT_REGENERATE_THEMES=1` to rewrite them. There is one resolver, so the
three apps cannot drift apart on a hex value.

`shared/themes/conformance/` holds resolution cases: input files plus the
expected resolved theme for each platform and appearance. They are written by
the core, and every client is held to them through its own bindings.

### The chosen theme follows you

Your themes and your choice of theme sync with the rest of the workspace
(see [sync](sync.md)):

- Every user theme is a row in the synced `themes` table (`id` is its
  identifier, `json` the file's text). On the Mac the themes folder stays the
  place you edit. Saving a file updates its row, and deleting the file deletes the
  row. A theme that arrives from another device is written into the folder as
  `<identifier>.json`, unless a file already claims that identifier. A theme
  whose identifier comes from its file name (`user.dusk`, from a `dusk.json`
  with no `identifier`) is written back under that name instead, `dusk.json`,
  so it is the same theme on the Mac. An edit from another device is
  written into the file if the file has not changed here since. If both
  changed, the Mac's file wins. A theme removed on another device moves its
  file to the Trash, unless the file has been edited here since. While any
  file in the folder is not valid JSON, the Mac writes and removes nothing,
  because a file half way through an edit does not say which theme it is. On the
  phones, **Settings → Theme → Import** adds a row from a `.json` file, and a
  theme can be removed there.
- The choice of theme and appearance (`theme.selected`, `theme.appearance`:
  `system`, `light` or `dark`) are rows in the synced `preferences` table. Each
  device can opt out with **Use a different theme on this device** (on the
  Mac, **Use a different theme on this Mac** in Settings → Appearance), which is
  stored locally and not synced. Turning it on keeps the theme the device is
  showing, and from then on changes stay on that device. Turning it off
  takes up the synced choice again. The first time a Mac with this feature opens
  the workspace, its current theme and appearance become the synced choice,
  unless another device has already set one.
- A device that does not know the chosen theme (it failed to load there, or
  has not arrived yet) shows Takt, the default, until it does.

### Your fonts are not part of the theme

On the Mac, **Settings → Appearance → Type** sets the interface, heading and
numeral/code families and a text size over whichever theme is active. They are
the reader's, not the theme's: they are not written into theme files, not
synced, and kept through a theme switch. **Reset to theme** clears them. A
chosen family is tried first and the theme's own `families` and `design` stay
behind it, so a family later uninstalled falls back to the theme rather than to
the system face; the text size multiplies `bodySize`, every `scale` step and the
micro-label's `size`, rounded to the half point. To ship a face *in* a theme,
name it in `structure.typography` as below.

## When something is wrong

A theme file is never all-or-nothing, and never stops the app starting.

- **A bad value** — a hex that is not a colour, an unknown `design` or
  `weight`, a negative size — is reported as an error and skipped; the value
  it would have replaced stays. The theme still loads.
- **An unknown key or role** — `"pallete"`, `"backgroud"` — is a warning and is
  ignored. It is worth reading these: a misspelt key otherwise does nothing,
  silently.
- **A file that is not JSON**, or has a string where a number goes, is skipped
  with the reason and the path (`structure.radius.panel should be a number`).
- **An `extends` that names nothing**, or a chain of them that loops, skips the
  file.

Errors appear on the window's status line and in **Diagnostics**, under
*Themes*; warnings go to Diagnostics only. The theme settings page lists them
all, per file.

Every theme that loads is also put through the same audit as the built-ins,
and the result is on the settings page under **Audit**: body text under 4.5:1
against the page is a warning, `primary` under 3:1 is a warning (it has to
carry the focus ring), and an accent under 4.5:1 is a *note* — fine for
headlines, fills and controls, not for a paragraph. Azure on Zed's paper is
3.6:1, which is why Zed itself has notes.

### Missing roles

With the default `extends`, every role always has a value. A theme with
`"extends": null` starts from nothing, so it has to give each role itself. A
role missing from one table is borrowed from the other and reported as an
error; a role missing from **both** would draw as debug magenta, so that theme
is skipped (`not loaded: no colour for categoricalPink`). Structure is never
missing: without a base it falls back to the default's.

## For developers

The format is read, merged, audited and written by the Rust core
(`core/src/theme/`: `json.rs` reads and writes the JSON dialect Swift's
`JSONDecoder` and `JSONEncoder` defined, `file.rs` the format, `merge.rs` and
`loader.rs` resolution, `audit.rs` the audit, `seeds.rs` the seed arithmetic,
`builtins.rs` the built-ins, `zed.rs` Zed's format, read into the same
files, `export.rs` and `conformance.rs` the files it writes) and tested there (`core/src/theme/tests.rs`). `TaktCore`
(`Sources/TaktCore/Theming/`, conversions in `Theme+Core.swift`) and Android's
`:core` (`core/theme/`, conversions in `ThemeCore.kt`) keep their own types
and call it once per folder or file; `corelogic-tests/ThemeFileTests.swift`
and `ThemeFileTest.kt` test it through them. What stays native is what a view
reads per render: a palette's role lookup and the hex parse list colours go
through.

- **Platforms.** `ThemeFileLoader.load(_:platform:)` resolves a folder for
  one `ThemePlatform` (`.macos`, `.ios`, `.android`). The default is `.macos`,
  so the Mac reads exactly as it did before platforms existed.
  `BuiltInThemeSpecifications.priority(for:)`, `chalk(for:)`, `chalkDark(for:)`,
  `defaultTheme(for:)`, `all(for:)` and `specification(withIdentifier:for:)`
  are the built-ins per platform. The phones' differences from the Mac are
  `priorityPlatformStructures` and `chalkPlatformStructures`, a partial
  `ThemeFile.Structure` per platform. Without a platform they are the Mac's.
  `defaultIdentifier` is the default's, still `native.theme.priority`.
- **Seeds.** `ThemeSeeds` grows a palette table from up to six colours; the
  loader applies a file's `seeds` after the base and before its `palette`. `ThemeStructureAudit` warns about a `touchTarget`
  between 0 and 44.
- **Shared files.** The core's `conformance.rs` builds the complete
  built-in files and the canonical resolved form (`ThemeConformance` in
  `TaktCore` is its Swift face), and
  `corelogic-tests/ThemeConformanceTests.swift` holds `shared/themes/` to it:
  the cases are listed in that test. Add a case there, then regenerate with
  `TAKT_REGENERATE_THEMES=1 swift test --filter TaktCoreTests.ThemeConformanceTests`.
  The core's own test rebuilds every committed case from its inputs.
  The format is in [`shared/themes/README.md`](../shared/themes/README.md).
- **Sync.** The rows are `WorkspaceStore.themes()`, `upsertTheme(id:json:)`,
  `deleteTheme(id:)`, `preference(_:)` and `setPreference(_:_:)`
  (`Sources/TaktWorkspace/WorkspaceStore+Themes.swift`, migration
  `v18_themes_and_preferences`). Writes that change nothing are skipped, which
  is what keeps a row and a file from echoing each other. The keys are
  `WorkspacePreferenceKey.themeSelected` and `.themeAppearance`.
- **The Mac.** `UserThemeLibrary` watches the folder and vends a
  `UserThemePlugin` per file to `ThemeManager`. Once the workspace is open it
  also mirrors the folder into the `themes` table on every load. What to do is
  decided by `ThemeFolderMirror.plan` in `TaktCore`, tested in
  `corelogic-tests/ThemeFolderMirrorTests.swift`. That compares each side
  with the digest of the text last mirrored, which is kept in `UserDefaults`.
  `ThemeChoiceSync` keeps `ThemeManager`'s pick and the appearance setting in
  step with the synced preferences, and owns the per-Mac opt-out.
  `AppDelegate` builds it once the workspace store exists. After a sync pull or
  a write from another process, `WorkspaceViewModel.reloadAfterExternalWrite`
  calls `onWorkspaceChangedElsewhere`, which reloads the themes and the
  choice.

See the Theme section of [`docs/plugins.md`](plugins.md).
