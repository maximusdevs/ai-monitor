# ai-monitor

See how much of your AI plan is left, for every account you use, in your panel.
Tracks the 5-hour and weekly windows of **Claude**, **Codex**, **Google
Antigravity (Gemini)**, Copilot, Cursor, Kiro, Z.AI, OpenRouter, DeepSeek, Kimi,
Grok and more, and notifies you when a window renews.

Runs on KDE Plasma 6, GNOME, Omarchy, Waybar, the Windows tray, the macOS menu
bar and in a terminal. All of them read the same `config.toml`.

**Author:** Luan Victor

![Three accounts side by side in the KDE panel](assets/screenshots/kde-panel-accounts.png)

## Install

```bash
cargo build --release
sudo make install                    # → /usr/local/bin
# or
make install PREFIX=$HOME/.local     # → ~/.local/bin
```

With Nix: `nix run github:maximusdevs/ai-monitor` (TUI: `#tui`).
On Windows, run `ai-monitor-tray.exe` (the build needs Node.js 20+).
Desktop frontends: [KDE](frontends/kde/README.md) ·
[GNOME](frontends/gnome/README.md) · [Omarchy](frontends/omarchy/README.md) ·
[macOS](frontends/macos/INSTALL.md) · [Windows](frontends/windows/README.md).

## Sign in

Claude and Codex reuse their CLI logins: run `claude` or `codex login` once.
Antigravity, Cursor and Kiro reuse their app logins. API-key providers read an
environment variable (e.g. `OPENROUTER_API_KEY`) or `api_key` in the config.
`ai-monitor detect` turns on every provider already signed in on this machine.

Each account you sign in to is picked up and keeps its own history. For more
than one Claude account:

```bash
ai-monitor account add work      # opens `claude` in a separate profile
```

## Use

```bash
ai-monitor                              # current usage in the terminal
ai-monitor-tui                          # every provider; press s for settings
ai-monitor usage --json                 # report for scripts and frontends
ai-monitor monitor --install-service    # renewal notifications, from login on
```

The monitor runs as a systemd user unit (Linux), a LaunchAgent (macOS) or a
sign-in entry (Windows).

## Display

The `[display]` section of `~/.config/ai-monitor/config.toml` controls every
frontend:

```toml
[display]
bar_mode = "expanded"     # expanded (side by side) | carousel (one at a time)
bar_count = 3             # items side by side (1-6)
bar_unit = "account"      # provider | account
carousel_interval = 5     # seconds per carousel step
hover_mode = "blocks"     # blocks (all at once) | pager (one, scroll to page)
show_account_name = true
```

Clicking always lists every account in use right now.

Waybar module:

```jsonc
"custom/aibar": {
    "exec": "ai-monitor --panel",
    "return-type": "json",
    "interval": 5,
    "on-click": "ai-monitor-tui",
    "on-scroll-up": "ai-monitor --panel-next",
    "on-scroll-down": "ai-monitor --panel-prev"
}
```

## More

- [Full guide](docs/guide.md): every provider, Waybar formats, theming, known issues
- [Configuration reference](docs/configuration.md)
- [Development](DEVELOPMENT.md) · [Changelog](CHANGELOG.md)

## License

MIT. See [LICENSE](LICENSE).
