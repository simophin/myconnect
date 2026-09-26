# Plan: localising the app

How the desktop app (`ferry-gui`, `src/ui/`) gets translated. Each phase
is done by one agent: read this plan and `HANDOFF.md`, do the phase,
pass the "done means" checks in `HANDOFF.md` (run under a private display
and bus, as `CLAUDE.md` says), tick the phase's boxes here, add a line
under "Notes" for anything the next phase must know, and commit.

## Decisions

- **Library: Fluent through `i18n-embed` + `i18n-embed-fl`**, the stack
  COSMIC (also iced) uses. `fl!` checks keys and arguments against the
  English file at compile time; Fluent handles CLDR plurals and word
  order; `DesktopLanguageRequester` reads the system language on all three
  platforms. Dependencies are optional, behind the `gui` feature, and are
  recorded in `adr/0001`'s library table (or a short ADR 0002,
  "Localisation").
- **What is translated:** the app: UI, tray, notifications, dialogs,
  error messages. **Stays English:** the CLI, the HTTP API (it reports
  error codes, which the UI words), logs, the website.
- **Language:** the system's; en-US is the fallback. A daemon setting
  overrides it (phase 7), since preferences live in the daemon
  (HANDOFF ground rules). `FERRY_LANG` forces one, for testing.
- **Right-to-left layout is out of scope.** Fluent's bidi isolation marks
  stay on so an RTL device name can't scramble a sentence.
- **First languages:** Simplified Chinese (`zh-CN`, CJK fonts, no spaces)
  and German (`de`, long words). They are written by an agent and need a
  native speaker's review; say so in the file headers.
- **Translations arrive as PRs** to `i18n/<lang>/ferry.ftl`. (Weblate,
  which reads Fluent, can be added later by the owner.)

## Rules for extraction

- One `i18n/<lang>/ferry.ftl` per language, grouped by feature with a
  comment header per group; keys are prefixed by feature (`browse-…`,
  `clipboard-…`, `error-<code>` for `ui::error`).
- Whole sentences only. No verb or phrase passed into a sentence
  (`describe_file_failures(verb, …)`, `"Couldn’t {action}"`): one key per
  case.
- Every count goes through a Fluent plural selector; no `len() == 1`
  branches choosing words.
- Device names, file names, paths and numbers are arguments, never
  concatenated with translated text.
- Tests run in en-US whatever the machine's `LANG` is, and with isolation
  marks off, so `find("To Pixel · 3.0 MB")` still matches.

## Phases

### 1. Infrastructure
- [x] `i18n.toml`, `i18n/en-US/ferry.ftl`, dependencies (gui-only),
      library table in `adr/0001`
- [x] `ui::i18n`: a static loader, `fl!` re-exported, language chosen at
      boot in `launch.rs` (system, `FERRY_LANG` override, en-US fallback)
- [x] Tests pinned to en-US with isolation marks off (`ui::testing`,
      `tests/ui_e2e.rs`, unit tests)
- [x] A test that every locale's `.ftl` parses and has exactly en-US's keys
- [x] `cargo build -p ferry` still has no iced, and no i18n crates

### 2. Extract every string
Split into four commits, one per step, each passing the checks.
- [ ] 2a. `ui::error` (codes to `error-<code>`), the shell, `widgets`,
      `actions`, `mod.rs`, `drops`, `launch`, and each feature's
      describe module
- [ ] 2b. Pages: devices, device, pairing, add device, transfers,
      settings, about
- [ ] 2c. Overlays (dialog, drop, incoming, toast) and features: share,
      clipboard, ping, findmyphone, battery, notifications, browse
- [ ] 2d. Tray menu, desktop notifications (incl. the "Open" action),
      file-dialog titles, `background.rs`; then a sweep:
      `grep` `src/ui` for remaining user-visible literals

### 3. Numbers, dates, sizes
- [ ] `format_timestamp` in the locale's date and time format (chrono's
      `unstable-locales`, or equivalent)
- [ ] `format_bytes` with the locale's decimal separator and unit names
      from the `.ftl`
- [ ] Percentages and other numbers through Fluent

### 4. Pseudo-locale
- [ ] A generated `en-XA` (accented, ~40% longer, bracketed) built from
      en-US, not checked in by hand
- [ ] Snapshot tests also render `en-XA` when `SNAPSHOT_DIR` is set; fix
      any unextracted string or clipped layout they show
- [ ] `FERRY_LANG=en-XA` works in the real app

### 5. Platform integration
- [ ] macOS: `CFBundleLocalizations` and a `<lang>.lproj` per shipped
      language in `packaging/macos`, so system dialogs follow the app;
      notification action titles localised
- [ ] Linux: `Name[xx]`/`Comment[xx]` in the `.desktop` file; the `.deb`
      recommends a CJK font
- [ ] Windows: installer languages in `installer.nsi`
- [ ] Check CJK glyphs render (no boxes) in the app under Xvfb

### 6. First languages and workflow
- [ ] `i18n/zh-CN/ferry.ftl` and `i18n/de/ferry.ftl`, complete
- [ ] Look at snapshots in both for clipping
- [ ] HANDOFF.md and ARCHITECTURE.md: how strings work, how to add a
      string and a language

### 7. Language setting
- [ ] A `language` daemon setting (none = system): core, `http.rs`,
      `client.rs`/CLI, then the Settings page, as the ground rules order
- [ ] Switching it re-selects the loader at run time, rebuilds the tray
      menu, and updates the window without a restart
- [ ] ARCHITECTURE.md updated for the API change

## Notes

Anything a later phase must know, one line each, newest last.

- Phase 1: `use crate::ui::i18n::fl;` then `fl!("key", name = value)`;
  `LOADER` is the static loader. Only `drop-send-to-header` (drops.rs's
  tray chooser, a plural) is extracted so far, as the pilot; 2a does the
  rest of `drops`.
- Language selection runs only in `launch::run`, never in `program`, so
  unit tests (`cfg(test)` turns isolation off) and `tests/ui_e2e.rs`
  (`ui::i18n::use_test_language()` in `launch`) stay en-US without marks.
  `set_use_isolating` resets whenever languages are (re)loaded: phase 7's
  runtime switch must apply it again after `select`.
- The key test (`ui::i18n::tests`) reads every `i18n/*/ferry.ftl` from
  disk and compares message, term and attribute ids with en-US; a new
  locale directory needs no registration.
- Editing only an `.ftl` doesn't make cargo rebuild `fl!`'s compile-time
  check (debug builds read the files at run time through `rust-embed`);
  touch a `.rs` file if a stale key check confuses you.
- `i18n_embed::select` negotiates with `Filtering`: check in phase 6 that
  macOS's `zh-Hans-CN` and Windows' tags reach `zh-CN`, or map them.
- Phase 1 was checked on macOS (no Xvfb there): `ui_e2e` is Linux-only and
  didn't run, the real-clipboard tests were skipped, and `tests/lan.rs`'s
  loopback test fails on macOS (no `127.255.255.255`, HANDOFF "Traps"; no gui code in it).
