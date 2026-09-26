# Changelog

All notable changes to **ai-monitor** are recorded here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Each release is also published at
<https://github.com/maximusdevs/ai-monitor/releases>.

## [Unreleased]

### Added

- **`[display]` panel layout, shared by every frontend.** `bar_mode`
  (`expanded` side by side, or `carousel` one at a time), `bar_count`,
  `bar_unit` (`provider` or `account`), `carousel_interval`, `hover_mode`
  (`blocks` or `pager`), `recent_only` and `hidden_accounts`. `usage --json`
  now carries a ready-to-draw `panel` object built from it, so no frontend
  re-derives the item order. Editable in the TUI settings overlay and through
  `settings show` / `settings apply`.
- `display.show_account_name` (default on): turn it off to show only
  provider names and usage in account mode. In the TUI settings, the KDE
  "Panel & accounts" page and every `--panel` frontend.
- **KDE plasmoid follows `[display]`.** The bar draws the report's `panel`
  (expanded or carousel; the wheel steps the carousel, hover pauses it), the
  tooltip shows every item in blocks or one at a time (pager, stepped by the
  wheel since a Plasma tooltip takes no clicks), and a click always lists
  every account in use right now. A new "Panel & accounts" page edits the
  shared settings through `settings apply`.
- **Waybar `--panel`.** One module that renders the same `[display]` panel:
  side by side or a carousel that advances with Waybar's `interval`;
  `--panel-next` / `--panel-prev` on the scroll wheel step the carousel and
  page the `pager` tooltip.
- **GNOME and Omarchy can draw the shared panel.** GNOME's new *Layout →
  Shared panel* runs `--panel --json` and scrolls with `--panel-next/prev`;
  Omarchy's new `sharedPanel` switch draws the report's `panel` as bar chips
  with the carousel on the wheel. Both are opt-in so existing bars keep
  their look.
- **Tray and popover.** The tray icon's hover tip is now the shared panel in
  one line per item (or the pager's current item), and the popover opens with
  an "In use now" card listing every account in use and its providers.
- **macOS menu bar: "Shared panel".** A new entry next to Overview draws
  the `[display]` panel from `--panel --json`, refreshing every 5 s so the
  carousel advances.
- **`ai-monitor monitor --install-service` / `--uninstall-service`.** Starts
  the renewal monitor at login and right away: a systemd user unit on Linux,
  a LaunchAgent on macOS, and a `HKCU\…\Run` sign-in entry on Windows (no
  administrator prompt). Before this the README called the monitor a user
  service, but nothing installed one on any platform.

### Changed

- **The project is now ai-monitor.** The binaries are `ai-monitor`,
  `ai-monitor-tui` and `ai-monitor-tray`; the config and cache live under
  `ai-monitor` directories. On first run the old `ai-usagebar` config, data
  and cache directories are copied over (paths inside `config.toml` are
  rewritten to the new directory), so existing accounts keep working. The
  desktop integrations have new ids: `io.github.maximusdevs.ai-monitor`
  (KDE), `ai-monitor@maximusdevs.github.io` (GNOME),
  `maximusdevs.ai-monitor` (Omarchy); reinstall them from this repository.
- The README is a shorter overview (features, display modes, install,
  sign-in, usage, configuration, credits to ai-usagebar and claudebar); the
  full reference moved to `docs/guide.md`.
- No CI on pushes or pull requests and no Dependabot: the `make test` gate
  runs locally. `release.yml` still builds a release when a `v*` tag is
  pushed.

- New README front section: display modes with screenshots (example
  accounts only), the `[display]` reference, a per-frontend table, and the
  project layout. This fork's author is credited next to the original
  copyright, which LICENSE keeps.
- **Repository layout.** Desktop frontends live under `frontends/` (`kde`,
  `gnome`, `omarchy`, `windows`, `macos`), packaging under `packaging/`
  (`aur`, `nix`), screenshots under `assets/screenshots/`, and the Rust
  sources are grouped into `src/core/`, `src/providers/` and `src/ui/`.
  Module paths are unchanged. A checkout that symlinks the plasmoid must
  point it at `frontends/kde/package`.
- The plasmoid no longer keeps its own copies of settings `config.toml`
  already owns (account mode, full email, extra models, refresh interval,
  quota notifications, account segregation and its carousel/count/hidden
  lists, "show all providers"). The old `notifyResets` toggle never did
  anything; `ui.notify_resets` is the switch the monitor reads.

- `ui.account_segregation` and `ui.segregated_accounts` are replaced by
  `display.bar_unit = "account"` and `display.hidden_accounts`. Neither old
  key shipped in a release.

### Fixed

- Account mode drew accounts that were only logged in, not in use, to fill
  `bar_count`. Only the active account and accounts with a current session
  appear now; the others are still monitored for renewals.

- **False quota-renewal alerts.** Metrics were paired by label alone, and
  Antigravity reports the same label for its 5h and weekly pools, so the
  monitor compared one window against the other and announced renewals that
  never happened. Windows are now paired by label and window length, both
  renewal producers share one rule, and the alert key includes the window.
- Dismissed renewals are remembered by the binary, so a cleared alert is not
  recorded again from a stale snapshot; sent and dismissed keys are pruned
  after 7 days.
- A `settings apply` that creates `[display]` keeps the file's trailing
  comments with the section they belong to.
- Cache writes no longer fall back to a non-atomic write when the atomic
  rename fails.
- **Windows renewal toasts were silently dropped.** They were posted under
  an app id Windows had never registered; they now use Windows PowerShell's
  registered one. The PowerShell that posts them no longer opens a console
  window, which used to take focus and close the tray popover.
- `account add` on Windows now finds Claude Code installed through npm
  (`claude.cmd`), not only the native `claude.exe`.
- `monitor --test` and several tests carried a real e-mail address; they use
  `user@example.com`.

[Unreleased]: https://github.com/maximusdevs/ai-monitor/commits/main
