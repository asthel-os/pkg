use crate::config::ConfigManager;
use crate::ui;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Deserialize)]
pub struct HookDef {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub events: Vec<String>,
    #[serde(default)]
    pub packages: Vec<String>,
    #[serde(default)]
    pub when: Option<String>,
    pub exec: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub ignore_failure: bool,
}

fn collect_hook_files(cfg: &ConfigManager) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if cfg.install_root == Path::new("/") {
        dirs.push(PathBuf::from("/etc/pkg/hooks.d"));
    }
    dirs.push(cfg.base_dir.join("hooks"));

    let mut files = Vec::new();
    for dir in dirs {
        if let Ok(entries) = fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("json") {
                    files.push(path);
                }
            }
        }
    }
    files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    files
}

fn matches_event(hook: &HookDef, event: &str) -> bool {
    if hook.events.is_empty() {
        return false;
    }
    hook.events.iter().any(|e| {
        let norm = e.trim().to_lowercase();
        norm == "*" || norm == "all" || norm == event
    })
}

fn matches_packages(hook: &HookDef, affected: &[String]) -> bool {
    if hook.packages.is_empty() {
        return true;
    }
    if hook.packages.iter().any(|p| p.trim() == "*") {
        return true;
    }
    affected
        .iter()
        .any(|pkg| hook.packages.iter().any(|pat| pat.trim() == pkg))
}

/// Runs all matching `pre` or `post` hooks for a `pkg` action (e.g. `install`, `upgrade`, `remove`, `update`).
pub fn run_hooks(
    cfg: &ConfigManager,
    when: &str,
    event: &str,
    affected_packages: &[String],
) -> Result<(), String> {
    let files = collect_hook_files(cfg);
    if files.is_empty() {
        return Ok(());
    }

    for path in files {
        let raw = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                ui::warn("HOOK", &format!("Could not read {}: {e}", path.display()));
                continue;
            }
        };

        let hook: HookDef = match serde_json::from_str(&raw) {
            Ok(h) => h,
            Err(e) => {
                ui::warn("HOOK", &format!("Invalid hook JSON {}: {e}", path.display()));
                continue;
            }
        };

        let hook_when = hook
            .when
            .as_deref()
            .unwrap_or("post")
            .trim()
            .to_lowercase();
        if hook_when != when {
            continue;
        }
        if !matches_event(&hook, event) || !matches_packages(&hook, affected_packages) {
            continue;
        }

        let hook_name = hook.name.clone().unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("hook")
                .to_string()
        });
        let label = hook
            .description
            .as_deref()
            .filter(|d| !d.trim().is_empty())
            .unwrap_or(&hook.exec);

        ui::info("HOOK", &format!("[{when}:{event}] {hook_name} — {label}"));

        let status = Command::new(&hook.exec)
            .args(&hook.args)
            .env("PKG_HOOK_EVENT", event)
            .env("PKG_HOOK_WHEN", when)
            .env("PKG_HOOK_PACKAGES", affected_packages.join(","))
            .env("PKG_INSTALL_ROOT", &cfg.install_root)
            .status();

        match status {
            Ok(st) if st.success() => {}
            Ok(st) => {
                let msg = format!(
                    "Hook '{hook_name}' ({}) exited with code {:?}",
                    hook.exec,
                    st.code()
                );
                if hook.ignore_failure {
                    ui::warn("HOOK", &msg);
                } else {
                    return Err(msg);
                }
            }
            Err(e) => {
                let msg = format!("Failed to execute hook '{hook_name}' ({}): {e}", hook.exec);
                if hook.ignore_failure {
                    ui::warn("HOOK", &msg);
                } else {
                    return Err(msg);
                }
            }
        }
    }

    Ok(())
}
