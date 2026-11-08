# Shared themes

Generated from the Rust core (`core/src/theme`), which every app resolves
themes with. Do not edit by hand: change `core/src/theme/` and run either

```bash
TAKT_REGENERATE_THEMES=1 cargo test --manifest-path core/Cargo.toml theme
TAKT_REGENERATE_THEMES=1 swift test --filter TaktCoreTests.ThemeConformanceTests
```

The core's own tests (`core/src/theme/tests.rs`) rebuild every file here from
its inputs and fail on any byte that differs; so do `ThemeConformanceTests`
through the Swift bindings, and Android's `ThemeConformanceTest` through the
Kotlin ones. A new case is added to the list in
`corelogic-tests/ThemeConformanceTests.swift`. The theme format itself is in
[`docs/themes.md`](../../docs/themes.md).

## `priority.json`, `chalk.json`, `chalk-dark.json`, `grape.json`, `one.json`

The built-ins as complete theme files. Every colour and every structural value
is stated, `extends` is `null`, `structure` holds the Mac's values, and
`platforms.ios` / `platforms.android` hold the phones' differences from them.
They resolve like any other file, so a reader needs no built-in of its own.
`priority.json` is the default; `chalk.json` and `chalk-dark.json` are Zed and
Zed Dark, under their old file names; `one.json` is Zed One, Zed's own One
Light and One Dark, which the Rust Mac app draws and writes as the synced
choice so the phones wear it too. It
loads these as its built-ins. The Android app copies them into its assets.

## `conformance/*.json`

One resolution case per file:

```json
{
  "files": [ { "name": "dusk.json", "json": "<the file's text>" } ],
  "selected": "user.dusk",
  "expected": {
    "macos":   { "light": <resolved>, "dark": <resolved> },
    "ios":     { "light": <resolved>, "dark": <resolved> },
    "android": { "light": <resolved>, "dark": <resolved> }
  },
  "issues": [ { "source": "dusk.json", "severity": "error", "message": "…" } ]
}
```

To run a case, load every entry of `files` together as one themes folder, with
the built-ins being Chalk and Chalk Dark resolved for the platform. A file's
`json` is its text, which may be invalid JSON on purpose. Then pick `selected`:
the user theme with that identifier if it loaded, otherwise the built-in with
that identifier, otherwise Chalk (the stand-in for a theme that did not load).
Resolve it for each platform. For each requested appearance, it must equal
`expected[platform][appearance]`.

`issues` is everything the load reports **except the audit** (contrast, radius
order, touch target and the rest of `validate()`), in order. That covers decoding
errors, unknown-key and platform-palette warnings, bad values, missing roles
and skipped files. It is the same on every platform, because a bad value in any
`platforms` entry is reported everywhere. A port must match `source` and
`severity` in order. `message` is the reference wording, and matching it exactly is
encouraged but not required.

### `<resolved>`: the canonical resolved theme

```json
{
  "identifier": "user.dusk",
  "name": "Dusk",
  "lockedAppearance": null,
  "appearance": "dark",
  "colors": { "paper": "#15131c", "mediaScrim": "#000000b3", "…": "…" },
  "structure": {
    "radius": { "panel": 0, "row": 0, "control": 2, "pill": 9999, "shell": 20 },
    "border": { "hairline": 1, "emphasis": 2, "focusRing": 2 },
    "spacing": { "xxs": 2, "xs": 4, "sm": 8, "md": 12, "lg": 16, "xl": 24 },
    "typography": {
      "display": { "families": ["IBM Plex Sans"], "design": "sans" },
      "body": { "families": ["IBM Plex Sans"], "design": "sans" },
      "mono": { "families": ["Lilex"], "design": "monospaced" },
      "bodySize": 13,
      "scale": { "caption": 12, "body": 13, "title": 15, "display": 28, "hero": 64 },
      "microLabel": { "size": 12, "weight": "regular", "tracking": 0, "uppercase": false, "role": "mutedText" }
    },
    "touchTarget": 0,
    "usesShadows": false,
    "usesGradientsOnChrome": false
  }
}
```

- `lockedAppearance` is always present: `"light"`, `"dark"` or `null`.
- `appearance` is the appearance actually drawn: the lock if there is one,
  otherwise the appearance requested.
- `colors` has every role, resolved in `appearance` the way the app draws it.
  `mediaLetterbox`, `mediaScrim` and `mediaScrimInk` always come from the
  light table. A role missing from the appearance's table is borrowed from
  the other one. Values are lowercase `#rrggbb`, or `#rrggbbaa` when alpha is
  not 1, with alpha rounded to the nearest of 255 steps.
- Every structural value is stated. Numbers are JSON numbers, and integral values are
  written without a fraction (`13`, not `13.0`), so compare them numerically.
- Keys are sorted and the files are pretty-printed with a trailing newline,
  but compare parsed JSON, not bytes.

## `zed/meadow.json`

Not generated: a small, original Zed theme family (Zed's own format, two
variants) that the core's tests load to hold the Zed mapping in
`core/src/theme/zed.rs` to `docs/themes.md` ("Zed themes"). Nothing loads it
as a built-in.
