// Author: Dustin Pilgrim
// License: GPLv3

use std::env;
use std::path::PathBuf;

use rune_cfg::RuneConfig;

#[derive(Debug, Clone, Copy)]
pub enum Theme {
    Auto,
    Dark,
    Light,
}

#[derive(Debug, Clone)]
pub struct CapitConfig {
    pub screenshot_directory: PathBuf,
    pub theme: Theme,
    pub accent_colour: u32,          // ARGB
    pub bar_background_colour: u32,  // ARGB
}

impl Default for CapitConfig {
    fn default() -> Self {
        Self {
            screenshot_directory: default_screenshot_dir(),
            theme: Theme::Auto,
            accent_colour: 0xFF0A_84FF,          // default blue
            bar_background_colour: 0xFF0F_1115,  // matches bar default
        }
    }
}

pub fn load() -> Result<CapitConfig, String> {
    let path = default_user_config_path();

    if !path.exists() {
        return Ok(CapitConfig::default());
    }

    let rc = RuneConfig::from_file(&path)
        .map_err(|e| format!("failed to read config: {e}"))?;

    parse_config(&rc)
}

fn parse_config(rc: &RuneConfig) -> Result<CapitConfig, String> {
    let mut cfg = CapitConfig::default();

    if !rc.has("capit") {
        return Ok(cfg);
    }

    // screenshot_directory
    if let Some(dir) = rc
        .get_optional::<String>("capit.screenshot_directory")
        .map_err(|e| format!("config error at capit.screenshot_directory: {e}"))?
    {
        cfg.screenshot_directory = expand_env(&dir);
    }

    // theme
    if let Some(theme_str) = rc
        .get_optional::<String>("capit.theme")
        .map_err(|e| format!("config error at capit.theme: {e}"))?
    {
        cfg.theme = match theme_str.trim().to_lowercase().as_str() {
            "auto" => Theme::Auto,
            "dark" => Theme::Dark,
            "light" => Theme::Light,
            other => {
                return Err(format!(
                    "config error at capit.theme: expected auto|dark|light, got \"{}\"",
                    other
                ))
            }
        };
    }

    // accent_colour
    if let Some(colour_str) = rc
        .get_optional::<String>("capit.accent_colour")
        .map_err(|e| format!("config error at capit.accent_colour: {e}"))?
    {
        cfg.accent_colour = parse_hex_colour(&colour_str)
            .map_err(|e| format!("config error at capit.accent_colour: {e}"))?;
    }

    // bar_background_colour
    if let Some(colour_str) = rc
        .get_optional::<String>("capit.bar_background_colour")
        .map_err(|e| format!("config error at capit.bar_background_colour: {e}"))?
    {
        cfg.bar_background_colour = parse_hex_colour(&colour_str)
            .map_err(|e| format!("config error at capit.bar_background_colour: {e}"))?;
    }

    Ok(cfg)
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

    let rgb = u32::from_str_radix(hex, 16)
        .map_err(|_| "invalid hex colour".to_string())?;

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

fn default_screenshot_dir() -> PathBuf {
    let home = env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join("Pictures").join("Screenshots")
}
