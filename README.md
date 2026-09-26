# ai-monitor

See how much of your AI plan is left, for every account you use, right in your
panel. ai-monitor tracks the 5-hour and weekly windows of **Claude**,
**Codex/ChatGPT** and **Google Antigravity (Gemini)**, plus Copilot, Cursor,
Kiro, Z.AI, OpenRouter, DeepSeek, Kimi, Grok and more, and sends a desktop
notification when a window renews.

It runs on KDE Plasma 6, GNOME, Omarchy, Waybar, the Windows tray, the macOS
menu bar and in a terminal. Every frontend reads the same `config.toml`, so
the panel looks and behaves the same everywhere.

**Author:** Luan Victor

![Three accounts side by side in the KDE panel: hugo (Gemini, Claude), elena (Codex) and felipe (Gemini), each with its 5-hour and weekly usage](assets/screenshots/kde-panel-accounts.png)

## Features

- **Every account, every provider.** Each account you sign in to is picked up
  automatically. An account can have one provider or several, and each keeps
  its own history, so switching accounts never overwrites another's figures.
- **Renewal alerts.** A background monitor watches every 5-hour and weekly
  window and notifies you once when it renews, even if it happened while the
  computer slept. Dismissed alerts stay dismissed.
- **Two bar layouts.** Everything side by side, or a carousel that shows one
  item at a time. Items can be providers or accounts.
- **Two hover styles.** All items in blocks, or one at a time with a pager.
  A click always lists every account in use right now.
- **Fast and safe.** Reads come from a cache (about 0.03 s), writes are
  atomic, a rate limit backs off, and the last good figures stay on screen
  when a provider is down.

## Display modes

`bar_mode = "expanded"` shows up to `bar_count` items side by side, here with
one item per provider:

![Gemini, Claude and Codex side by side](assets/screenshots/kde-panel-providers.png)

`bar_mode = "carousel"` shows one item at a time and moves on every
`carousel_interval` seconds. Scroll over the bar to step it yourself:

![Carousel, first step: hugo with Gemini and Claude](assets/screenshots/kde-panel-carousel-1.png)
![Carousel, next step: elena with Codex](assets/screenshots/kde-panel-carousel-2.png)

| Frontend | Bar | Hover | Turn it on |
|---|---|---|---|
| KDE Plasma 6 | expanded / carousel, wheel steps | blocks / pager | on by default |
| Waybar | expanded / carousel | blocks / pager (wheel) | `exec: ai-monitor --panel` |
| GNOME Shell | expanded / carousel | dropdown lists all | Preferences → Layout → Shared panel |
| Omarchy | expanded / carousel, wheel steps | summary tooltip | bar setting `sharedPanel` |
| Windows tray | hover tip + "In use now" card | one line per item | on by default |
| macOS menu bar | expanded / carousel | hover tip + dropdown | Switch provider → Shared panel |

## Install

From source (Linux, macOS):

```bash
cargo build --release
sudo make install                    # → /usr/local/bin
# or
make install PREFIX=$HOME/.local     # → ~/.local/bin
```

This installs `ai-monitor` (CLI and Waybar widget) and `ai-monitor-tui`.

- **Nix:** `nix run github:maximusdevs/ai-monitor` (TUI: `#tui`).
- **Windows:** `cargo build --release` also builds `ai-monitor-tray.exe`, the
  tray popover (needs Node.js 20+). See [Windows](frontends/windows/README.md).
- **Desktop frontends:** [KDE](frontends/kde/README.md) ·
  [GNOME](frontends/gnome/README.md) · [Omarchy](frontends/omarchy/README.md) ·
  [macOS](frontends/macos/INSTALL.md).

## Sign in

| Provider | How |
|---|---|
| Claude | Run `claude` once; tokens refresh on their own |
| Codex | Run `codex login` once |
| Google Antigravity | The running app, or the Google session it saved |
| Cursor, Kiro, Kimi, SuperGrok | Their existing app or CLI login |
| Copilot | `gh auth login --web` |
| Z.AI, OpenRouter, DeepSeek, Grok, … | An API key in an env var (e.g. `OPENROUTER_API_KEY`) or `api_key` in the config |

`ai-monitor detect` turns on every provider already signed in on this machine
(it only reads local files and keychains, never the network). `ai-monitor
vendors` lists every provider and whether it is ready.

For more than one Claude account:

```bash
ai-monitor account add work      # opens `claude` in a separate profile
ai-monitor account list
```

## Use

```bash
ai-monitor                              # current usage in the terminal
ai-monitor --vendor openai              # one provider
ai-monitor usage                        # every provider and account
ai-monitor usage --json                 # the same, for scripts and frontends
ai-monitor-tui                          # tabs per provider; s = settings, r = refresh
```

Renewal notifications:

```bash
ai-monitor monitor --install-service    # start at login, and now
ai-monitor monitor --uninstall-service
ai-monitor monitor --test               # send a sample notification
```

The service is a systemd user unit on Linux, a LaunchAgent on macOS and a
sign-in entry on Windows. Notifications use `notify-send`, Notification
Center and Windows toasts.

## Configuration

The file is `~/.config/ai-monitor/config.toml` (`%APPDATA%\ai-monitor\config.toml`
on Windows). You can also edit it from the TUI settings (`s`) or the KDE
"Panel & accounts" page.

```toml
[display]
bar_mode = "expanded"     # expanded | carousel
bar_count = 3             # items side by side (1-6)
bar_unit = "account"      # provider | account
carousel_interval = 5     # seconds per carousel step (2-300)
hover_mode = "blocks"     # blocks | pager
show_account_name = true  # off: only provider names and usage
recent_only = false       # account mode: only each account's recent providers
hidden_accounts = []      # accounts left out of the bar

[ui]
notify_resets = true      # renewal notifications
```

Waybar module:

```jsonc
"custom/aibar": {
    "exec": "ai-monitor --panel",
    "return-type": "json",
    "interval": 5,            // match carousel_interval for the carousel
    "signal": 13,
    "tooltip": true,
    "on-click": "ai-monitor-tui",
    "on-scroll-up": "ai-monitor --panel-next",
    "on-scroll-down": "ai-monitor --panel-prev"
}
```

Coming from ai-usagebar? Your config, accounts and cache are copied over
the first time ai-monitor runs.

## More

- [Full guide](docs/guide.md): every provider, Waybar formats, theming, known issues
- [Configuration reference](docs/configuration.md)
- [Provider endpoints](docs/vendor-endpoints.md)
- [Development](DEVELOPMENT.md) · [Changelog](CHANGELOG.md)

## Credits

ai-monitor is my adaptation of two projects to the way I use AI tools day to
day:

- [**ai-usagebar**](https://github.com/akitaonrails/ai-usagebar) by Fabio Akita
  (AkitaOnRails) and its contributors, the multi-provider widget this code
  started from.
- [**claudebar**](https://github.com/mryll/claudebar) by mryll, the original
  Claude usage bar that ai-usagebar was ported from. The tooltip style,
  severity colors and pacing math come from it.

Thank you to both.

## License

MIT. See [LICENSE](LICENSE).
