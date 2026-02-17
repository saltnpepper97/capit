# Capit

Capit is a Wayland-native screenshot tool built around a daemon + IPC architecture with clean overlay UIs.

- **capitd** → daemon (owns Wayland, overlays, capture, config, notifications)
- **capit** → client CLI (requests capture + spawns UIs)
- **capit-bar** → floating bar UI executable (Region / Screen / Window)
- Overlays built with smithay-client-toolkit (SCTK) + wlr-layer-shell
- UI theme (accent + bar background) provided by daemon via IPC

---

## Features

- Region capture (drag to select)
- Screen picker overlay (monitor selection)
- Floating bar UI (`capit-bar`) (Region / Screen / Window)
- Configurable UI theme (accent + bar background)
- Desktop notifications on success/error
- Clean modular Rust architecture
- Safe fallback to internal defaults on config errors

---

## Build

From the workspace root:

```bash
cargo build --release
```

Binaries:

```
target/release/capitd
target/release/capit
target/release/capit-bar
```

---

## Run

Start daemon:

```bash
./target/release/capitd
```

Use client:

```bash
./target/release/capit bar          # spawns ./target/release/capit-bar
./target/release/capit region
./target/release/capit screen
./target/release/capit screen -o DP-1
./target/release/capit outputs
./target/release/capit status
```

---

## Configuration

Capit supports **user config, system-wide config, and internal defaults**.

### Priority Order

1. **User config**
   ```
   ~/.config/capit/capit.rune
   ```
   or
   ```
   $XDG_CONFIG_HOME/capit/capit.rune
   ```

2. **System-wide config**
   Recommended location:
   ```
   /etc/capit/capit.rune
   ```

3. **Internal defaults** (compiled into daemon)

If:
- Config file is missing → internal defaults are used.
- Config file exists but contains invalid fields → those fields fall back to defaults and a warning is logged.
- Entire config fails to parse → daemon logs warning and runs with defaults.

Daemon never crashes due to config errors.

---

## Example Config (`~/.config/capit/capit.rune`)

```rune
@author "Dustin Pilgrim"
@description "Capit configuration"

capit:
  screenshot_directory "$env.HOME/Pictures/Screenshots"
  theme "auto"              # auto | dark | light
  accent_colour "#0A84FF"   # RRGGBB
  bar_background_colour "#0F1115"
end
```

### Supported Fields

- `screenshot_directory` → where screenshots are saved
- `theme` → auto | dark | light
- `accent_colour` → hex colour (#RRGGBB)
- `bar_background_colour` → hex colour (#RRGGBB)

---

## Output Directory Resolution

Screenshots are saved using this priority:

1. `$CAPIT_DIR` (if set)
2. `screenshot_directory` from config
3. `$XDG_RUNTIME_DIR`
4. `/tmp`

Filename format:

```
capit-<unix_timestamp>.png
```

---

## Roadmap

### High Priority
- Proper system-wide config fallback (`/etc/capit/capit.rune`)
- Per-field config validation with soft fallback
- Window capture (portal + PipeWire → PNG)

### Next
- Clipboard copy support
- Theme polish (accent applied to overlay outlines; more bar customization, including item background)
- Minor UI refinements

### Later
- Recording mode
- Performance tuning

---

## License

MIT © Dustin Pilgrim
