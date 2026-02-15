// Author: Dustin Pilgrim
// License: GPLv3

use std::env;
use std::path::{Path, PathBuf};

use eventline::warn;
use rune_cfg::RuneConfig;

#[derive(Debug, Clone)]
pub struct CapitConfig {
    pub screenshot_directory: PathBuf,
    pub accent_colour: u32,          // ARGB
    pub bar_background_colour: u32,  // ARGB
}

impl Default for CapitConfig {
    fn default() -> Self {
        Self {
            screenshot_directory: default_screenshot_dir(),
            accent_colour: 0xFF0A_84FF,          // default blue
            bar_background_colour: 0xFF0F_1115,  // matches bar default
        }
    }
}

/// Load config with this priority:
/// 1) User config:   $XDG_CONFIG_HOME/capit/capit.rune  (or ~/.config/capit/capit.rune)
/// 2) System config: $XDG_CONFIG_DIRS/capit/capit.rune  (plus /etc/capit/capit.rune fallback)
/// 3) Defaults
///
/// Any invalid option falls back to defaults and logs a warning.
pub fn load() -> Result<CapitConfig, String> {
    let (cfg, _src) = load_with_source()?;
    Ok(cfg)
}

/// Same as `load()`, but also returns which path was used (if any).
pub fn load_with_source() -> Result<(CapitConfig, Option<PathBuf>), String> {
    // 1) user
    let user = default_user_config_path();
    if user.exists() {
        return load_from_path(&user).map(|c| (c, Some(user)));
    }

    // 2) system (XDG_CONFIG_DIRS + /etc/capit fallback)
    for sys in system_config_paths() {
        if sys.exists() {
            return load_from_path(&sys).map(|c| (c, Some(sys)));
        }
    }

    // 3) defaults
    Ok((CapitConfig::default(), None))
}

fn load_from_path(path: &Path) -> Result<CapitConfig, String> {
    let rc = RuneConfig::from_file(path).map_err(|e| format!("failed to read config {}: {e}", path.display()))?;
    Ok(parse_config(&rc))
}

fn parse_config(rc: &RuneConfig) -> CapitConfig {
    let mut cfg = CapitConfig::default();

    if !rc.has("capit") {
        return cfg;
    }

    // screenshot_directory
    match rc.get_optional::<String>("capit.screenshot_directory") {
        Ok(Some(dir)) => {
            let p = expand_env(&dir);
            if p.as_os_str().is_empty() {
                warn!("config: capit.screenshot_directory is empty; using default {}", cfg.screenshot_directory.display());
            } else {
                cfg.screenshot_directory = p;
            }
        }
        Ok(None) => {}
        Err(e) => warn!("config: invalid capit.screenshot_directory ({e}); using default {}", cfg.screenshot_directory.display()),
    }

    // accent_colour
    match rc.get_optional::<String>("capit.accent_colour") {
        Ok(Some(colour_str)) => match parse_hex_colour(&colour_str) {
            Ok(v) => cfg.accent_colour = v,
            Err(e) => warn!("config: invalid capit.accent_colour ({e}); using default 0x{:08X}", cfg.accent_colour),
        },
        Ok(None) => {}
        Err(e) => warn!("config: invalid capit.accent_colour ({e}); using default 0x{:08X}", cfg.accent_colour),
    }

    // bar_background_colour
    match rc.get_optional::<String>("capit.bar_background_colour") {
        Ok(Some(colour_str)) => match parse_hex_colour(&colour_str) {
            Ok(v) => cfg.bar_background_colour = v,
            Err(e) => warn!(
                "config: invalid capit.bar_background_colour ({e}); using default 0x{:08X}",
                cfg.bar_background_colour
            ),
        },
        Ok(None) => {}
        Err(e) => warn!(
            "config: invalid capit.bar_background_colour ({e}); using default 0x{:08X}",
            cfg.bar_background_colour
        ),
    }

    cfg
}

fn parse_hex_colour(s: &str) -> Result<u32, String> {
    let s = s.trim();

    if !s.starts_with('#') {
        return Err("colour must start with #".into());
    }

    let hex = &s[1..];

    if hex.len() != 6 {
        return Err("colour must be 6 hex digits (RRGGBB)".into());
    }

    let rgb = u32::from_str_radix(hex, 16).map_err(|_| "invalid hex colour".to_string())?;

    Ok(0xFF00_0000 | rgb)
}

fn expand_env(s: &str) -> PathBuf {
    let mut out = s.to_string();

    if out.contains("$env.HOME") {
        if let Ok(home) = env::var("HOME") {
            out = out.replace("$env.HOME", &home);
        }
    }

    PathBuf::from(out)
}

fn default_user_config_path() -> PathBuf {
    let dir: PathBuf = if let Ok(xdg) = env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg)
    } else {
        let home = env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(".config")
    };

    dir.join("capit").join("capit.rune")
}

/// System-wide config search:
/// - Each dir in $XDG_CONFIG_DIRS gets: <dir>/capit/capit.rune
/// - Plus a direct fallback: /etc/capit/capit.rune
fn system_config_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();

    if let Ok(dirs) = env::var("XDG_CONFIG_DIRS") {
        for d in dirs.split(':').filter(|s| !s.trim().is_empty()) {
            out.push(PathBuf::from(d).join("capit").join("capit.rune"));
        }
    } else {
        // common default for XDG_CONFIG_DIRS is /etc/xdg
        out.push(PathBuf::from("/etc/xdg").join("capit").join("capit.rune"));
    }

    // explicit fallback you asked for
    out.push(PathBuf::from("/etc").join("capit").join("capit.rune"));

    out
}

fn default_screenshot_dir() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join("Pictures").join("Screenshots")
}
