use crate::config::ConfigManager;
use crate::i18n::Lang;
use crate::models::{MasterManifest, RegisteredSource, RepoEntry};
use crate::ui::{self, BOLD, CYAN, DIM, RESET};
use std::fs;
use std::path::Path;
use std::process::Command;

fn detect_target_os_version() -> Option<String> {
    if let Ok(v) = std::env::var("PKG_VERSION") {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    let os_rel_path = if let Ok(root) = std::env::var("PKG_INSTALL_ROOT") {
        let p = Path::new(&root).join("etc/os-release");
        if p.exists() {
            p
        } else {
            Path::new("/etc/os-release").to_path_buf()
        }
    } else {
        Path::new("/etc/os-release").to_path_buf()
    };

    if let Ok(os_rel) = fs::read_to_string(&os_rel_path) {
        let mut is_asthel = false;
        let mut ver_id = None;
        for line in os_rel.lines() {
            let trimmed = line.trim();
            if let Some(val) = trimmed.strip_prefix("ID=") {
                if val.trim_matches('"') == "asthel-os" {
                    is_asthel = true;
                }
            } else if let Some(val) = trimmed.strip_prefix("VERSION_ID=") {
                let v = val.trim_matches('"').trim();
                if !v.is_empty() {
                    ver_id = Some(v.to_string());
                }
            }
        }
        if is_asthel && ver_id.is_some() {
            return ver_id;
        }
    }
    None
}

fn compare_versions_desc(a: &str, b: &str) -> std::cmp::Ordering {
    let parse_parts = |s: &str| -> Vec<(u64, String)> {
        s.split('.')
            .map(|p| (p.parse::<u64>().unwrap_or(0), p.to_string()))
            .collect()
    };
    let pa = parse_parts(a);
    let pb = parse_parts(b);
    pb.cmp(&pa)
}

fn find_latest_local_version(dir: &Path) -> Option<String> {
    let versions_dir = dir.join("versions");
    let entries = fs::read_dir(&versions_dir).ok()?;
    let mut candidates: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir() && e.path().join("repository.json").exists())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    candidates.sort_by(|a, b| compare_versions_desc(a, b));
    candidates.into_iter().next()
}

fn find_latest_github_version(slug: &str) -> Result<String, String> {
    let api_url = format!("https://api.github.com/repos/{slug}/contents/versions");
    let out = Command::new("curl")
        .args([
            "-sLf",
            "--connect-timeout",
            "8",
            "--max-time",
            "20",
            "--retry",
            "1",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: asthelos-pkg/0.1.0",
            "-H",
            "Cache-Control: no-cache",
            &api_url,
        ])
        .output()
        .map_err(|e| format!("Failed to execute curl: {e}"))?;

    if !out.status.success() || out.stdout.is_empty() {
        return Err(format!("Could not list versions/ in '{slug}'"));
    }

    let items: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|_| format!("Invalid GitHub API response for '{slug}/versions'"))?;
    let arr = items
        .as_array()
        .ok_or_else(|| format!("Unexpected GitHub API format for '{slug}/versions'"))?;

    let mut dirs: Vec<String> = arr
        .iter()
        .filter(|item| item.get("type").and_then(|t| t.as_str()) == Some("dir"))
        .filter_map(|item| {
            item.get("name")
                .and_then(|n| n.as_str())
                .map(|s| s.to_string())
        })
        .collect();
    dirs.sort_by(|a, b| compare_versions_desc(a, b));
    dirs.into_iter()
        .next()
        .ok_or_else(|| format!("No version directories found in '{slug}/versions'"))
}

pub fn resolve_master_manifest_local(dir: &Path) -> Result<MasterManifest, String> {
    if let Some(ver) = detect_target_os_version() {
        let ver_dir = dir.join("versions").join(&ver);
        if ver_dir.join("repository.json").exists() {
            return ConfigManager::load_manifest_from_dir(&ver_dir);
        }
        if dir.join("versions").is_dir() {
            return Err(format!(
                "Version '{ver}' not found in '{}/versions'",
                dir.display()
            ));
        }
    } else if let Some(latest) = find_latest_local_version(dir) {
        let ver_dir = dir.join("versions").join(&latest);
        return ConfigManager::load_manifest_from_dir(&ver_dir);
    }

    ConfigManager::load_manifest_from_dir(dir)
}

pub fn resolve_master_manifest_github(slug: &str) -> Result<MasterManifest, String> {
    if let Some(ver) = detect_target_os_version() {
        let ver_path = format!("versions/{ver}/repository.json");
        if let Ok(ver_raw) = fetch_github_raw_file(slug, &ver_path) {
            return serde_json::from_str::<MasterManifest>(&ver_raw)
                .map_err(|_| format!("Invalid {ver_path} in master repo '{slug}'"));
        }
    } else if let Ok(latest) = find_latest_github_version(slug) {
        let ver_path = format!("versions/{latest}/repository.json");
        if let Ok(ver_raw) = fetch_github_raw_file(slug, &ver_path) {
            return serde_json::from_str::<MasterManifest>(&ver_raw)
                .map_err(|_| format!("Invalid {ver_path} in master repo '{slug}'"));
        }
    }

    // Backwards-compatible fallback for master repos with a root repository.json
    let raw_json = fetch_github_raw_file(slug, "repository.json")?;
    let manifest = serde_json::from_str::<MasterManifest>(&raw_json)
        .map_err(|_| "Invalid repository.json in master repo".to_string())?;

    if let Some(ver) = manifest.version.as_deref() {
        let ver_path = format!("versions/{ver}/repository.json");
        if let Ok(ver_raw) = fetch_github_raw_file(slug, &ver_path) {
            if let Ok(mut ver_manifest) = serde_json::from_str::<MasterManifest>(&ver_raw) {
                if ver_manifest.name.is_empty() {
                    ver_manifest.name = manifest.name;
                }
                return Ok(ver_manifest);
            }
        }
    }
    Ok(manifest)
}

/// `sudo pkg add github <target>` or `sudo pkg add git <target>`
pub fn add_github(cfg: &ConfigManager, target: &str) -> Result<(), String> {
    cfg.require_root(Lang::detect())?;
    let local_path = Path::new(target);
    if local_path.exists() {
        let canon = local_path
            .canonicalize()
            .map_err(|e| format!("Invalid path '{}': {e}", target))?;
        let manifest = resolve_master_manifest_local(&canon)?;
        let loc = canon.to_string_lossy().to_string();
        return register_master_manifest(cfg, manifest, &loc, &loc);
    }

    let slug = normalize_github_slug(target);
    if !slug.contains('/') {
        return Err(format!(
            "Invalid GitHub repository '{target}'. Expected '<owner>/<repo>' or a GitHub URL."
        ));
    }

    // Check if the GitHub repository contains `versions/<ver>/repository.json` (or a root `repository.json`)
    if let Ok(manifest) = resolve_master_manifest_github(&slug) {
        let repo_url = format!("https://github.com/{slug}");
        return register_master_manifest(cfg, manifest, &slug, &repo_url);
    }

    // Otherwise, register as a direct GitHub Releases repository (`<owner>/<repo>`)
    let (namespace, repo_name) = slug
        .split_once('/')
        .map(|(o, r)| (o.to_string(), r.to_string()))
        .unwrap();

    let entry = RepoEntry {
        name: repo_name.clone(),
        repo_type: "git".to_string(),
        url: None,
        suite: None,
        components: Vec::new(),
        repo: Some(slug.clone()),
        path: None,
    };

    let mut db = cfg.load_db();
    if let Some(existing) = db
        .sources
        .iter_mut()
        .find(|s| s.namespace == namespace && s.origin_kind == "git")
    {
        existing.repos.retain(|r| r.name != repo_name);
        existing.repos.push(entry);
    } else {
        db.sources.push(RegisteredSource {
            namespace: namespace.clone(),
            slug: slug.clone(),
            origin_kind: "git".to_string(),
            location: format!("https://github.com/{slug}/releases"),
            repos: vec![entry],
        });
    }

    cfg.save_db(&db)?;
    ui::success(
        "ADD:GIT",
        &format!(
            "Added GitHub Releases repository '{BOLD}{CYAN}{namespace}/{repo_name}{RESET}' (https://github.com/{slug}/releases)"
        ),
    );
    Ok(())
}

/// `sudo pkg add ppa <ppa_target>`
pub fn add_ppa(cfg: &ConfigManager, ppa_target: &str, suite: Option<&str>) -> Result<(), String> {
    cfg.require_root(Lang::detect())?;
    let clean = ppa_target.trim_start_matches("ppa:");
    let (owner, name) = clean.split_once('/').ok_or_else(|| {
        "Invalid PPA format. Use: pkg add ppa <owner>/<ppa-name> or ppa:<owner>/<ppa-name>"
            .to_string()
    })?;

    let ppa_str = format!("ppa:{owner}/{name}");
    let suite_val = suite
        .map(|s| s.to_string())
        .unwrap_or_else(detect_system_codename);

    let entry = RepoEntry {
        name: name.to_string(),
        repo_type: "ppa".to_string(),
        url: Some(format!(
            "https://ppa.launchpadcontent.net/{owner}/{name}/ubuntu"
        )),
        suite: Some(suite_val.clone()),
        components: vec!["main".to_string()],
        repo: Some(ppa_str.clone()),
        path: None,
    };

    let mut db = cfg.load_db();
    if let Some(existing) = db
        .sources
        .iter_mut()
        .find(|s| s.namespace == owner && s.origin_kind == "ppa")
    {
        existing.repos.retain(|r| r.name != name);
        existing.repos.push(entry);
    } else {
        db.sources.push(RegisteredSource {
            namespace: owner.to_string(),
            slug: clean.to_string(),
            origin_kind: "ppa".to_string(),
            location: ppa_str.clone(),
            repos: vec![entry],
        });
    }

    cfg.save_db(&db)?;
    ui::success(
        "ADD:PPA",
        &format!(
            "Added PPA repository '{BOLD}{CYAN}{owner}/{name}{RESET}' ({ppa_str}, suite: {suite_val})"
        ),
    );
    Ok(())
}

/// `sudo pkg add deb <mirror>` or `sudo pkg add <mirror>` (both are identical)
pub fn add_deb_mirror(
    cfg: &ConfigManager,
    mirror_url: &str,
    suite: Option<&str>,
    components: &[String],
    custom_name: Option<&str>,
) -> Result<(), String> {
    cfg.require_root(Lang::detect())?;
    let inferred_name = custom_name
        .map(|s| s.to_string())
        .unwrap_or_else(|| infer_mirror_name(mirror_url));

    let suite_val = suite
        .map(|s| s.to_string())
        .unwrap_or_else(detect_system_codename);

    let comps = if components.is_empty() {
        vec!["main".to_string()]
    } else {
        components.to_vec()
    };

    let entry = RepoEntry {
        name: inferred_name.clone(),
        repo_type: "deb".to_string(),
        url: Some(mirror_url.trim_end_matches('/').to_string()),
        suite: Some(suite_val.clone()),
        components: comps.clone(),
        repo: None,
        path: None,
    };

    let mut db = cfg.load_db();
    let ns = "deb".to_string();
    if let Some(existing) = db
        .sources
        .iter_mut()
        .find(|s| s.namespace == ns && s.origin_kind == "deb")
    {
        existing.repos.retain(|r| r.name != inferred_name);
        existing.repos.push(entry);
    } else {
        db.sources.push(RegisteredSource {
            namespace: ns.clone(),
            slug: format!("{ns}/{inferred_name}"),
            origin_kind: "deb".to_string(),
            location: mirror_url.to_string(),
            repos: vec![entry],
        });
    }

    cfg.save_db(&db)?;
    ui::success(
        "ADD:DEB",
        &format!(
            "Added DEB mirror '{BOLD}{CYAN}{ns}/{inferred_name}{RESET}' -> {mirror_url} (suite: {suite_val}, components: {})",
            comps.join(" ")
        ),
    );
    Ok(())
}

fn register_master_manifest(
    cfg: &ConfigManager,
    manifest: MasterManifest,
    slug: &str,
    location: &str,
) -> Result<(), String> {
    let mut db = cfg.load_db();
    db.sources
        .retain(|s| s.namespace != manifest.name && s.slug != slug);
    db.sources.push(RegisteredSource {
        namespace: manifest.name.clone(),
        slug: slug.to_string(),
        origin_kind: "master-json".to_string(),
        location: location.to_string(),
        repos: manifest.repos.clone(),
    });
    cfg.save_db(&db)?;

    ui::success(
        "PKG-MASTER",
        &format!(
            "Added '{BOLD}{}{RESET}' ({slug}) · {} repositories",
            manifest.name,
            manifest.repos.len()
        ),
    );
    for r in &manifest.repos {
        let full = r.full_name(&manifest.name);
        let detail = match r.normalized_type() {
            "git" => {
                if let Some(p) = &r.path {
                    format!("git · {p}")
                } else if let Some(rp) = &r.repo {
                    format!("git · {rp}")
                } else {
                    format!("git · {}/packages.json", r.name)
                }
            }
            "ppa" => format!("ppa · {}", r.repo.as_deref().unwrap_or("")),
            _ => format!(
                "deb · {} {}",
                r.url.as_deref().unwrap_or(""),
                r.suite.as_deref().unwrap_or("")
            ),
        };
        println!("  {DIM}•{RESET} {BOLD}{full}{RESET} {DIM}{detail}{RESET}");
    }
    Ok(())
}

pub fn list_repos(cfg: &ConfigManager) {
    let db = cfg.load_db();
    if db.sources.is_empty() {
        ui::warn(
            "REPO",
            "No repositories configured. Add one using: sudo pkg add github <owner>/<repo> or sudo pkg add <mirror>",
        );
        return;
    }

    println!("{BOLD}Configured repositories:{RESET}");
    for src in &db.sources {
        let slug_info = if !src.slug.is_empty() && src.slug != src.location {
            format!("{} • {}", src.slug, src.location)
        } else {
            src.location.clone()
        };
        println!(
            "\n{BOLD}{}{RESET} {DIM}({}: {}){RESET}",
            src.namespace, src.origin_kind, slug_info
        );
        for r in &src.repos {
            let full = r.full_name(&src.namespace);
            let kind = r.normalized_type();
            let target = match kind {
                "git" => {
                    if let Some(p) = &r.path {
                        p.clone()
                    } else if let Some(rp) = &r.repo {
                        format!("github.com/{rp}/releases")
                    } else {
                        format!("{}/packages.json", r.name)
                    }
                }
                "ppa" => r.repo.clone().unwrap_or_else(|| r.name.clone()),
                _ => format!(
                    "{} [{}]",
                    r.url.as_deref().unwrap_or(""),
                    r.suite.as_deref().unwrap_or("stable")
                ),
            };
            println!(
                "  {DIM}├─{RESET} {BOLD}{full}{RESET} {DIM}{kind} · {target}{RESET}"
            );
        }
    }
}

pub fn remove_repo(cfg: &ConfigManager, target: &str) -> Result<(), String> {
    cfg.require_root(Lang::detect())?;
    let mut db = cfg.load_db();
    let norm_slug = normalize_github_slug(target);

    let mut removed_namespaces = Vec::new();
    db.sources.retain(|s| {
        let matched = s.namespace == target
            || s.slug == target
            || s.slug == norm_slug
            || s.location == target;
        if matched {
            removed_namespaces.push(s.namespace.clone());
        }
        !matched
    });

    if !removed_namespaces.is_empty() {
        cfg.save_db(&db)?;
        // Purge all indexed packages and cached lists belonging to the removed master repository
        let mut index = cfg.load_index();
        index.retain(|p| {
            !removed_namespaces
                .iter()
                .any(|ns| p.origin == *ns || p.origin.starts_with(&format!("{ns}/")))
        });
        let _ = cfg.save_index(&index);
        for ns in &removed_namespaces {
            purge_cached_lists_for_prefix(&cfg.lists_dir, &format!("{ns}_"));
        }
        ui::success(
            "REMOVE",
            &format!("Repository '{target}' and all its indexed sub-repositories removed."),
        );
        return Ok(());
    }

    let mut removed_sub_origins = Vec::new();
    for src in &mut db.sources {
        src.repos.retain(|r| {
            let full = r.full_name(&src.namespace);
            let matched = full == target || r.name == target || r.repo.as_deref() == Some(&norm_slug);
            if matched {
                removed_sub_origins.push(full);
            }
            !matched
        });
    }
    db.sources.retain(|s| !s.repos.is_empty());

    if !removed_sub_origins.is_empty() {
        cfg.save_db(&db)?;
        let mut index = cfg.load_index();
        index.retain(|p| !removed_sub_origins.contains(&p.origin));
        let _ = cfg.save_index(&index);
        for orig in &removed_sub_origins {
            purge_cached_lists_for_prefix(&cfg.lists_dir, &orig.replace('/', "_"));
        }
        ui::success("REMOVE", &format!("Sub-repository '{target}' removed."));
        Ok(())
    } else {
        Err(format!("Repository '{target}' not found."))
    }
}

pub fn purge_cached_lists_for_prefix(lists_dir: &Path, prefix: &str) {
    if let Ok(entries) = fs::read_dir(lists_dir) {
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_string();
            if fname.starts_with(prefix) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

pub fn init_master(dir: &Path, name: &str) -> Result<(), String> {
    fs::create_dir_all(dir.join(name)).map_err(|e| e.to_string())?;
    let sample = MasterManifest {
        name: name.to_string(),
        version: None,
        repos: vec![
            RepoEntry {
                name: name.to_string(),
                repo_type: "git".to_string(),
                url: None,
                suite: None,
                components: Vec::new(),
                repo: None,
                path: Some(format!("{name}/packages.json")),
            },
            RepoEntry {
                name: "drivers".to_string(),
                repo_type: "ppa".to_string(),
                url: None,
                suite: None,
                components: Vec::new(),
                repo: Some("ppa:graphics-drivers/ppa".to_string()),
                path: None,
            },
        ],
    };
    let json = serde_json::to_string_pretty(&sample).map_err(|e| e.to_string())?;
    let path = dir.join("repository.json");
    fs::write(&path, json).map_err(|e| e.to_string())?;
    ui::success(
        "INIT",
        &format!("Created master repository template at {}", path.display()),
    );
    Ok(())
}

pub fn normalize_github_slug(input: &str) -> String {
    let s = input
        .trim()
        .trim_end_matches('/')
        .trim_end_matches("/HEAD/repository.json")
        .trim_end_matches("/main/repository.json")
        .trim_end_matches(".git")
        .trim_start_matches("https://raw.githubusercontent.com/")
        .trim_start_matches("https://github.com/")
        .trim_start_matches("http://github.com/")
        .trim_start_matches("git@github.com:");
    s.to_string()
}

/// Fetches a file directly from a GitHub repository bypassing the 5-minute `raw.githubusercontent.com` CDN cache
pub fn fetch_github_raw_file(slug: &str, file_path: &str) -> Result<String, String> {
    let clean_path = file_path.trim_start_matches('/');
    let api_url = format!("https://api.github.com/repos/{slug}/contents/{clean_path}");

    if let Ok(out) = Command::new("curl")
        .args([
            "-sLf",
            "--connect-timeout",
            "8",
            "--max-time",
            "20",
            "--retry",
            "1",
            "-H",
            "Accept: application/vnd.github.raw+json",
            "-H",
            "User-Agent: asthelos-pkg/0.1.0",
            "-H",
            "Cache-Control: no-cache",
            &api_url,
        ])
        .output()
    {
        if out.status.success() && !out.stdout.is_empty() {
            return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
        }
    }

    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let raw_url = format!("https://raw.githubusercontent.com/{slug}/HEAD/{clean_path}?cb={ts}");

    let out = Command::new("curl")
        .args([
            "-sLf",
            "--connect-timeout",
            "8",
            "--max-time",
            "20",
            "--retry",
            "1",
            "-H",
            "User-Agent: asthelos-pkg/0.1.0",
            "-H",
            "Cache-Control: no-cache, no-store, must-revalidate",
            &raw_url,
        ])
        .output()
        .map_err(|e| format!("Failed to execute curl: {e}"))?;

    if out.status.success() && !out.stdout.is_empty() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "Master repository '{slug}' or '{clean_path}' not found on GitHub"
        ))
    }
}

fn infer_mirror_name(url: &str) -> String {
    let trimmed = url
        .trim_end_matches('/')
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let parts: Vec<&str> = trimmed.split('/').collect();
    if let Some(last) = parts.last() {
        if !last.is_empty() && !last.contains('.') {
            return last.to_string();
        }
    }
    parts
        .first()
        .unwrap_or(&"mirror")
        .split('.')
        .next()
        .unwrap_or("mirror")
        .to_string()
}

pub fn detect_system_codename() -> String {
    if let Ok(content) = fs::read_to_string("/etc/os-release") {
        for line in content.lines() {
            if let Some(val) = line.strip_prefix("UBUNTU_CODENAME=") {
                let clean = val.trim().trim_matches('"');
                if !clean.is_empty() {
                    return clean.to_string();
                }
            }
        }
        for line in content.lines() {
            if let Some(val) = line.strip_prefix("VERSION_CODENAME=") {
                let clean = val.trim().trim_matches('"');
                if !clean.is_empty() {
                    return clean.to_string();
                }
            }
        }
    }
    "resolute".to_string()
}
