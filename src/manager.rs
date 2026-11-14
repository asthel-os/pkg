use crate::config::{is_root, ConfigManager};
use crate::hooks;
use crate::i18n::Lang;
use crate::models::{GitPackagesManifest, IndexedPackage, InstalledPackage, MasterManifest};
use crate::repo::{
    detect_system_codename, fetch_github_raw_file, normalize_github_slug,
    purge_cached_lists_for_prefix,
};
use crate::ui::{self, BOLD, CYAN, DIM, GREEN, MAGENTA, RED, RESET, YELLOW};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

/// `sudo pkg update` — Fast parallel repository synchronization with original AsthelOS pkg UI
pub fn cmd_update(cfg: &ConfigManager) -> Result<(), String> {
    let lang = Lang::detect();
    cfg.require_root(lang)?;
    hooks::run_hooks(cfg, "pre", "update", &[])?;

    let mut db = cfg.load_db();

    if db.sources.is_empty() {
        ui::warn(
            "UPDATE",
            "No repositories configured. Add a repository first (e.g., 'sudo pkg add github <owner>/<repo>' or 'sudo pkg add <mirror>').",
        );
        return Ok(());
    }

    let mut tasks = Vec::new();
    let mut idx = 1usize;

    // Directly verify every master-json repository (e.g., asthel-os/pkg-master) before syncing sub-repos!
    // If the master repository or its repository.json no longer exists, none of its sub-repos (like asthelos/ubuntu) will be synced.
    for src in &mut db.sources {
        if src.origin_kind == "master-json" {
            let inferred_slug = if !src.slug.is_empty() && src.slug.contains('/') {
                src.slug.clone()
            } else {
                normalize_github_slug(&src.location)
            };

            let live_manifest: Result<MasterManifest, String> =
                if src.location.starts_with("http://") || src.location.starts_with("https://") {
                    crate::repo::resolve_master_manifest_github(&inferred_slug)
                } else {
                    crate::repo::resolve_master_manifest_local(Path::new(&src.location))
                };

            match live_manifest {
                Ok(manifest) => {
                    src.namespace = manifest.name;
                    if src.slug.is_empty() {
                        src.slug = inferred_slug;
                    }
                    src.repos = manifest.repos;
                }
                Err(reason) => {
                    ui::err_line(idx, &inferred_slug, "repository.json", &reason);
                    idx += 1;
                    purge_cached_lists_for_prefix(&cfg.lists_dir, &format!("{}_", src.namespace));
                    continue;
                }
            }
        }

        for repo in &src.repos {
            tasks.push((
                idx,
                repo.full_name(&src.namespace),
                repo.normalized_type().to_string(),
                repo.clone(),
                src.origin_kind.clone(),
                src.slug.clone(),
                src.location.clone(),
            ));
            idx += 1;
        }
    }

    let total_tasks = tasks.len();
    let results_mutex: Mutex<Vec<(usize, Vec<IndexedPackage>)>> = Mutex::new(Vec::new());
    let progress_counter = Mutex::new(0usize);

    ui::draw_progress_bar(lang, 0, total_tasks);

    std::thread::scope(|s| {
        for (counter, full_display, kind, repo, origin_kind, src_slug, src_location) in tasks {
            let results_ref = &results_mutex;
            let prog_ref = &progress_counter;
            s.spawn(move || {
                let pkgs = match kind.as_str() {
                    "git" => {
                        let uses_packages_json = repo.path.is_some()
                            || (origin_kind == "master-json" && repo.repo.is_none());

                        if uses_packages_json {
                            let rel_path = repo
                                .path
                                .clone()
                                .unwrap_or_else(|| format!("{}/packages.json", repo.name));
                            match fetch_git_channel_packages(
                                &full_display,
                                &rel_path,
                                &src_slug,
                                &src_location,
                            ) {
                                Ok((total_defined, released_pkgs)) => {
                                    ui::hit_line(
                                        counter,
                                        &full_display,
                                        "packages.json",
                                        &format!(
                                            "type: git • {} pkgs ({} releases ready)",
                                            total_defined,
                                            released_pkgs.len()
                                        ),
                                    );
                                    released_pkgs
                                }
                                Err(reason) => {
                                    ui::err_line(counter, &full_display, &rel_path, &reason);
                                    Vec::new()
                                }
                            }
                        } else {
                            let gh_slug = repo.repo.as_deref().unwrap_or(&repo.name);
                            match fetch_github_release_package(&full_display, &repo.name, gh_slug) {
                                Ok(pkg) => {
                                    ui::get_line(
                                        counter,
                                        &full_display,
                                        &format!("releases/{}", pkg.version),
                                        &format!("type: git • github.com/{gh_slug}"),
                                    );
                                    vec![pkg]
                                }
                                Err(reason) => {
                                    ui::err_line(counter, &full_display, "releases/latest", &reason);
                                    Vec::new()
                                }
                            }
                        }
                    }
                    "ppa" => {
                        let ppa_ref = repo.repo.as_deref().unwrap_or(&repo.name);
                        let clean_ppa = ppa_ref.trim_start_matches("ppa:");
                        let base_url = repo.url.clone().unwrap_or_else(|| {
                            format!("https://ppa.launchpadcontent.net/{clean_ppa}/ubuntu")
                        });
                        let suite = repo.suite.clone().unwrap_or_else(detect_system_codename);
                        let comps = if repo.components.is_empty() {
                            vec!["main".to_string()]
                        } else {
                            repo.components.clone()
                        };

                        match fetch_deb_repository_packages(
                            cfg,
                            &full_display,
                            "ppa",
                            &base_url,
                            &suite,
                            &comps,
                        ) {
                            Ok(pkgs) => {
                                ui::hit_line(
                                    counter,
                                    &full_display,
                                    &suite,
                                    &format!("type: ppa • {} pkgs", pkgs.len()),
                                );
                                pkgs
                            }
                            Err(reason) => {
                                ui::err_line(counter, &full_display, &suite, &reason);
                                Vec::new()
                            }
                        }
                    }
                    _ => {
                        let base_url = repo.url.as_deref().unwrap_or("").trim_end_matches('/');
                        let suite = repo.suite.clone().unwrap_or_else(detect_system_codename);
                        let comps = if repo.components.is_empty() {
                            vec!["main".to_string()]
                        } else {
                            repo.components.clone()
                        };

                        match fetch_deb_repository_packages(
                            cfg,
                            &full_display,
                            "deb",
                            base_url,
                            &suite,
                            &comps,
                        ) {
                            Ok(pkgs) => {
                                ui::hit_line(
                                    counter,
                                    &full_display,
                                    &suite,
                                    &format!("type: deb • {}", comps.join(", ")),
                                );
                                pkgs
                            }
                            Err(reason) => {
                                ui::err_line(counter, &full_display, &suite, &reason);
                                Vec::new()
                            }
                        }
                    }
                };

                if let Ok(mut done) = prog_ref.lock() {
                    *done += 1;
                    ui::draw_progress_bar(lang, *done, total_tasks);
                }
                if let Ok(mut guard) = results_ref.lock() {
                    guard.push((counter, pkgs));
                }
            });
        }
    });

    let mut collected = results_mutex.into_inner().unwrap_or_default();
    collected.sort_by_key(|(c, _)| *c);
    let new_index: Vec<IndexedPackage> = collected.into_iter().flat_map(|(_, p)| p).collect();

    cfg.save_db(&db)?;
    cfg.save_index(&new_index)?;
    ui::finish_progress_bar(lang);

    ui::success("SYNC", &lang.sync_done(total_tasks, new_index.len()));
    hooks::run_hooks(cfg, "post", "update", &[])?;
    Ok(())
}

/// Fetches a grouped `<channel>/packages.json` (e.g., `asthel-os/packages.json`) from the master repo
/// and resolves the latest `.deb` GitHub Release for each package inside it under a single repository line (e.g., `asthel-os/asthel-os`).
fn fetch_git_channel_packages(
    origin_display: &str,
    rel_path: &str,
    src_slug: &str,
    src_location: &str,
) -> Result<(usize, Vec<IndexedPackage>), String> {
    let clean_rel = rel_path.trim_start_matches('/');
    let manifest_raw = if src_location.starts_with("http://") || src_location.starts_with("https://")
    {
        let slug = if !src_slug.is_empty() && src_slug.contains('/') {
            src_slug.to_string()
        } else {
            normalize_github_slug(src_location)
        };
        fetch_github_raw_file(&slug, clean_rel)?
    } else {
        let full_path = Path::new(src_location).join(clean_rel);
        fs::read_to_string(&full_path)
            .map_err(|_| format!("File '{}' not found", full_path.display()))?
    };

    let parsed: GitPackagesManifest = serde_json::from_str(&manifest_raw)
        .map_err(|e| format!("Invalid JSON in {clean_rel}: {e}"))?;

    let total_defined = parsed.packages.len();
    let mut resolved = Vec::new();

    for item in parsed.packages {
        if let Ok(pkg) = fetch_github_release_package(origin_display, &item.name, &item.repo) {
            resolved.push(pkg);
        }
    }

    Ok((total_defined, resolved))
}

fn fetch_github_release_package(
    origin_display: &str,
    pkg_name: &str,
    gh_slug: &str,
) -> Result<IndexedPackage, String> {
    let latest_url = format!("https://api.github.com/repos/{gh_slug}/releases/latest");
    let list_url = format!("https://api.github.com/repos/{gh_slug}/releases?per_page=1");

    let release_json = fetch_json_url(&latest_url).or_else(|_| {
        fetch_json_url(&list_url).and_then(|v| {
            v.as_array()
                .and_then(|arr| arr.first().cloned())
                .ok_or_else(|| "No releases found".to_string())
        })
    })?;

    let tag = release_json
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "No release tag found".to_string())?;

    let version = tag.trim_start_matches('v').to_string();
    let desc = release_json
        .get("name")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| release_json.get("body").and_then(|v| v.as_str()))
        .unwrap_or("GitHub Release package")
        .lines()
        .next()
        .unwrap_or("GitHub Release package")
        .trim()
        .to_string();

    let arch = detect_system_arch();
    let mut chosen_url = String::new();
    let mut chosen_name = String::new();
    let mut chosen_size = 0u64;

    if let Some(assets) = release_json.get("assets").and_then(|v| v.as_array()) {
        // Select ONLY .deb package assets from the latest GitHub Release matching the system architecture
        let deb_assets: Vec<&Value> = assets
            .iter()
            .filter(|a| {
                a.get("name")
                    .and_then(|v| v.as_str())
                    .map(|n| n.to_lowercase().ends_with(".deb"))
                    .unwrap_or(false)
            })
            .collect();

        let best = deb_assets
            .iter()
            .find(|a| {
                let n = a
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_lowercase();
                n.contains(&format!("_{arch}.deb"))
                    || n.contains(&format!("-{arch}.deb"))
                    || n.contains(&arch)
            })
            .or_else(|| {
                deb_assets.iter().find(|a| {
                    let n = a
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_lowercase();
                    n.contains("_all.deb") || n.contains("-all.deb")
                })
            })
            .or_else(|| {
                if deb_assets.len() == 1 {
                    deb_assets.first()
                } else {
                    None
                }
            });

        if let Some(asset) = best {
            chosen_url = asset
                .get("browser_download_url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            chosen_name = asset
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(pkg_name)
                .to_string();
            chosen_size = asset.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
        }
    }

    if chosen_url.is_empty() {
        return Err(format!(
            "Latest release ({tag}) has no .deb package for architecture '{arch}'"
        ));
    }

    Ok(IndexedPackage {
        name: pkg_name.to_string(),
        version,
        origin: origin_display.to_string(),
        repo_type: "git".to_string(),
        architecture: arch,
        description: desc,
        download_url: chosen_url,
        asset_name: chosen_name,
        size: chosen_size,
        installed_size: chosen_size * 3,
        sha256: String::new(),
        maintainer: gh_slug.to_string(),
        section: "github".to_string(),
        dependencies: Vec::new(),
    })
}

fn fetch_json_url(url: &str) -> Result<Value, String> {
    let out = Command::new("curl")
        .args([
            "-sLf",
            "--max-time",
            "6",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "User-Agent: asthelos-pkg/0.1.0",
            url,
        ])
        .output()
        .map_err(|e| format!("Failed to execute curl: {e}"))?;

    if !out.status.success() || out.stdout.is_empty() {
        return Err("No release found or unreachable".to_string());
    }

    serde_json::from_slice::<Value>(&out.stdout).map_err(|e| format!("Invalid JSON response: {e}"))
}

fn resolve_deb_mirror_for_arch(base_url: &str, arch: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if arch != "amd64"
        && arch != "i386"
        && (trimmed.contains("archive.ubuntu.com/ubuntu")
            || trimmed.contains("security.ubuntu.com/ubuntu"))
    {
        return "http://ports.ubuntu.com/ubuntu-ports".to_string();
    }
    trimmed.to_string()
}

fn fetch_deb_repository_packages(
    cfg: &ConfigManager,
    origin_display: &str,
    repo_type: &str,
    base_url: &str,
    suite: &str,
    components: &[String],
) -> Result<Vec<IndexedPackage>, String> {
    let arch = detect_system_arch();
    let clean_base_owned = resolve_deb_mirror_for_arch(base_url, &arch);
    let clean_base = clean_base_owned.as_str();
    let comp_results: Mutex<Vec<IndexedPackage>> = Mutex::new(Vec::new());

    std::thread::scope(|s| {
        for comp in components {
            let comp_results_ref = &comp_results;
            let arch_ref = &arch;
            s.spawn(move || {
                let comp_slug = format!(
                    "{}_{}_{}_{}",
                    origin_display.replace('/', "_"),
                    suite,
                    comp,
                    arch_ref
                );
                let txt_cache = cfg.lists_dir.join(format!("{comp_slug}_Packages"));
                let xz_cache = cfg.lists_dir.join(format!("{comp_slug}_Packages.xz"));
                let gz_cache = cfg.lists_dir.join(format!("{comp_slug}_Packages.gz"));
                let xz_url =
                    format!("{clean_base}/dists/{suite}/{comp}/binary-{arch_ref}/Packages.xz");
                let gz_url =
                    format!("{clean_base}/dists/{suite}/{comp}/binary-{arch_ref}/Packages.gz");

                let mut parsed_text: Option<String> = None;

                if download_file_conditional(&xz_url, &xz_cache, 20) {
                    if let Ok(out) = Command::new("xz")
                        .args(["-dc", &xz_cache.to_string_lossy()])
                        .output()
                    {
                        if out.status.success() && !out.stdout.is_empty() {
                            let s_txt = String::from_utf8_lossy(&out.stdout).into_owned();
                            let _ = fs::write(&txt_cache, &s_txt);
                            parsed_text = Some(s_txt);
                        }
                    }
                }

                if parsed_text.is_none() && download_file_conditional(&gz_url, &gz_cache, 20) {
                    if let Ok(out) = Command::new("gzip")
                        .args(["-dc", &gz_cache.to_string_lossy()])
                        .output()
                    {
                        if out.status.success() && !out.stdout.is_empty() {
                            let s_txt = String::from_utf8_lossy(&out.stdout).into_owned();
                            let _ = fs::write(&txt_cache, &s_txt);
                            parsed_text = Some(s_txt);
                        }
                    }
                }

                if parsed_text.is_none() && repo_type == "ppa" && suite != "noble" {
                    let fallback_gz_url =
                        format!("{clean_base}/dists/noble/{comp}/binary-{arch_ref}/Packages.gz");
                    if download_file_conditional(&fallback_gz_url, &gz_cache, 12) {
                        if let Ok(out) = Command::new("gzip")
                            .args(["-dc", &gz_cache.to_string_lossy()])
                            .output()
                        {
                            if out.status.success() && !out.stdout.is_empty() {
                                let s_txt = String::from_utf8_lossy(&out.stdout).into_owned();
                                let _ = fs::write(&txt_cache, &s_txt);
                                parsed_text = Some(s_txt);
                            }
                        }
                    }
                }

                if parsed_text.is_none() && txt_cache.exists() {
                    parsed_text = fs::read_to_string(&txt_cache).ok();
                }

                if parsed_text.is_none() {
                    parsed_text = read_local_apt_lists_fallback(suite, comp, arch_ref);
                    if let Some(ref txt) = parsed_text {
                        let _ = fs::write(&txt_cache, txt);
                    }
                }

                if let Some(raw_stanzas) = parsed_text {
                    let parsed = parse_debian_packages_index(
                        &raw_stanzas,
                        origin_display,
                        repo_type,
                        clean_base,
                    );
                    if let Ok(mut guard) = comp_results_ref.lock() {
                        guard.extend(parsed);
                    }
                }
            });
        }
    });

    let packages = comp_results.into_inner().unwrap_or_default();
    if packages.is_empty() {
        return Err(format!("Could not fetch index from {clean_base}/dists/{suite}"));
    }

    Ok(packages)
}

fn read_local_apt_lists_fallback(suite: &str, comp: &str, arch: &str) -> Option<String> {
    let lists_dir = Path::new("/var/lib/apt/lists");
    if !lists_dir.exists() {
        return None;
    }
    let entries = fs::read_dir(lists_dir).ok()?;
    let suffix_plain = format!("dists_{suite}_{comp}_binary-{arch}_Packages");
    let suffix_lz4 = format!("dists_{suite}_{comp}_binary-{arch}_Packages.lz4");

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.ends_with(&suffix_plain) {
            if let Ok(txt) = fs::read_to_string(entry.path()) {
                return Some(txt);
            }
        } else if name.ends_with(&suffix_lz4) {
            if let Ok(out) = Command::new("lz4")
                .args(["-dc", &entry.path().to_string_lossy()])
                .output()
            {
                if out.status.success() && !out.stdout.is_empty() {
                    return Some(String::from_utf8_lossy(&out.stdout).into_owned());
                }
            }
        }
    }
    None
}

pub fn parse_debian_packages_index(
    content: &str,
    origin_display: &str,
    repo_type: &str,
    base_url: &str,
) -> Vec<IndexedPackage> {
    let mut out = Vec::with_capacity(8_000);

    for block in content.split("\n\n") {
        if block.trim().is_empty() {
            continue;
        }
        let mut name = String::new();
        let mut version = String::new();
        let mut arch = String::new();
        let mut desc = String::new();
        let mut filename = String::new();
        let mut size = 0u64;
        let mut installed_size_kb = 0u64;
        let mut sha256 = String::new();
        let mut maintainer = String::new();
        let mut section = String::new();
        let mut deps = Vec::new();

        for line in block.lines() {
            if let Some(v) = line.strip_prefix("Package: ") {
                name = v.trim().to_string();
            } else if let Some(v) = line.strip_prefix("Version: ") {
                version = v.trim().to_string();
            } else if let Some(v) = line.strip_prefix("Architecture: ") {
                arch = v.trim().to_string();
            } else if let Some(v) = line.strip_prefix("Description: ") {
                desc = v.trim().to_string();
            } else if let Some(v) = line.strip_prefix("Filename: ") {
                filename = v.trim().trim_start_matches('/').to_string();
            } else if let Some(v) = line.strip_prefix("Size: ") {
                size = v.trim().parse::<u64>().unwrap_or(0);
            } else if let Some(v) = line.strip_prefix("Installed-Size: ") {
                installed_size_kb = v.trim().parse::<u64>().unwrap_or(0);
            } else if let Some(v) = line.strip_prefix("SHA256: ") {
                sha256 = v.trim().to_string();
            } else if let Some(v) = line.strip_prefix("Maintainer: ") {
                maintainer = v.trim().to_string();
            } else if let Some(v) = line.strip_prefix("Section: ") {
                section = v.trim().to_string();
            } else if let Some(v) = line
                .strip_prefix("Depends: ")
                .or_else(|| line.strip_prefix("Pre-Depends: "))
            {
                for part in v.split(',') {
                    let first_alt = part.split('|').next().unwrap_or("").trim();
                    let dep_pkg = first_alt
                        .split_whitespace()
                        .next()
                        .unwrap_or("")
                        .split(':')
                        .next()
                        .unwrap_or("")
                        .trim();
                    if !dep_pkg.is_empty() && !deps.contains(&dep_pkg.to_string()) {
                        deps.push(dep_pkg.to_string());
                    }
                }
            }
        }

        if !name.is_empty() && !version.is_empty() && !filename.is_empty() {
            let asset_name = filename.rsplit('/').next().unwrap_or(&name).to_string();
            let inst_bytes = if installed_size_kb > 0 {
                installed_size_kb * 1024
            } else {
                size * 3
            };
            out.push(IndexedPackage {
                name,
                version,
                origin: origin_display.to_string(),
                repo_type: repo_type.to_string(),
                architecture: arch,
                description: desc,
                download_url: format!("{base_url}/{filename}"),
                asset_name,
                size,
                installed_size: inst_bytes,
                sha256,
                maintainer,
                section,
                dependencies: deps,
            });
        }
    }

    out
}

/// `sudo pkg install <packages...>` — Original AsthelOS `pkg` UI with tree-view dependency breakdown and progress bar
pub fn cmd_install(
    cfg: &ConfigManager,
    packages: &[String],
    yes: bool,
    reinstall: bool,
    no_recommends: bool,
    fix_broken: bool,
    download_only: bool,
) -> Result<(), String> {
    cmd_install_with_event(
        cfg,
        packages,
        yes,
        reinstall,
        no_recommends,
        fix_broken,
        download_only,
        "install",
    )
}

fn cmd_install_with_event(
    cfg: &ConfigManager,
    packages: &[String],
    yes: bool,
    reinstall: bool,
    _no_recommends: bool,
    _fix_broken: bool,
    download_only: bool,
    event_name: &str,
) -> Result<(), String> {
    let lang = Lang::detect();
    cfg.require_root(lang)?;

    if packages.is_empty() {
        return Err("Specify at least one package to install.".to_string());
    }

    let mut db = cfg.load_db();
    let index = cfg.load_index();
    if index.is_empty() {
        return Err(
            "Package index is empty. Add a repository and run 'sudo pkg update' first.".to_string(),
        );
    }

    ui::info("DEPS", lang.resolving_deps());

    let sys_installed = cfg.read_system_dpkg_installed();
    let index_map: HashMap<&str, &IndexedPackage> =
        index.iter().map(|p| (p.name.as_str(), p)).collect();

    let mut plan: Vec<IndexedPackage> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut requested_names: HashSet<String> = HashSet::new();

    for req in packages {
        let target = find_package_in_index(&index, req)
            .ok_or_else(|| lang.unable_to_locate(req))?;
        requested_names.insert(target.name.clone());

        let is_installed = db.installed.iter().any(|i| i.name == target.name)
            || sys_installed.contains_key(&target.name);

        if is_installed && !reinstall && !download_only {
            // Even if target itself is already installed, verify all its recursive dependencies are installed too
            for dep_name in &target.dependencies {
                let dep_inst = db.installed.iter().any(|i| i.name == *dep_name)
                    || sys_installed.contains_key(dep_name);
                if !dep_inst {
                    if let Some(dep_pkg) = index_map.get(dep_name.as_str()) {
                        resolve_dependencies_recursive(
                            dep_pkg,
                            &index_map,
                            &db.installed,
                            &sys_installed,
                            &mut visited,
                            &mut plan,
                        );
                    }
                }
            }
            ui::info("OK", &lang.already_newest(&target.name, &target.version));
            continue;
        }

        resolve_dependencies_recursive(
            &target,
            &index_map,
            &db.installed,
            &sys_installed,
            &mut visited,
            &mut plan,
        );
    }

    if plan.is_empty() {
        return Ok(());
    }

    let targets: Vec<&IndexedPackage> = plan
        .iter()
        .filter(|p| requested_names.contains(&p.name))
        .collect();
    let deps: Vec<&IndexedPackage> = plan
        .iter()
        .filter(|p| !requested_names.contains(&p.name))
        .collect();

    let total_dl_bytes: u64 = plan.iter().map(|p| p.size).sum();
    let total_inst_bytes: u64 = plan.iter().map(|p| p.installed_size).sum();
    let (lbl_inst, _lbl_up, _lbl_rem, lbl_dl) = lang.summary_labels();
    let lbl_disk = lang.disk_impact_label(false);

    // Minimalist AsthelOS transaction summary
    println!("\n{BOLD}{}{RESET}", lang.plan_title_install());
    println!("  {DIM}{}{RESET}", lang.target_packages_label(targets.len()));
    for t in &targets {
        println!(
            "    {GREEN}•{RESET} {BOLD}{}{RESET} {DIM}{} · {} · {}{RESET}",
            t.name,
            t.version,
            t.origin,
            format_bytes(t.size)
        );
    }

    if !deps.is_empty() {
        println!("  {DIM}{}{RESET}", lang.dependencies_label(deps.len()));
        for (i, d) in deps.iter().enumerate() {
            let branch = if i + 1 == deps.len() { "└─" } else { "├─" };
            println!(
                "    {DIM}{branch}{RESET} {BOLD}{}{RESET} {DIM}{} · {} · {}{RESET}",
                d.name,
                d.version,
                d.origin,
                format_bytes(d.size)
            );
        }
    }

    println!(
        "\n  {GREEN}+{}{RESET} {DIM}{lbl_inst} · {lbl_dl}: {}{} · {lbl_disk}: +{}{RESET}\n",
        plan.len(),
        format_bytes(total_dl_bytes),
        "",
        format_bytes(total_inst_bytes)
    );

    if !yes && !confirm_prompt(lang)? {
        ui::warn("ABORT", lang.abort_msg());
        return Ok(());
    }

    let total_steps = if download_only {
        plan.len()
    } else {
        plan.len() * 3
    };
    let mut current_step = 0usize;
    let mut downloaded_paths: Vec<(IndexedPackage, PathBuf)> = Vec::new();

    ui::draw_progress_bar(lang, 0, total_steps);

    for (idx, pkg) in plan.iter().enumerate() {
        current_step += 1;
        ui::action_line(
            "Get",
            YELLOW,
            idx + 1,
            &pkg.origin,
            &pkg.name,
            &pkg.version,
            &format!(
                "type: {} • {}",
                pkg.repo_type,
                if pkg.size > 0 {
                    format_bytes(pkg.size)
                } else {
                    "release".to_string()
                }
            ),
        );
        ui::draw_progress_bar(lang, current_step, total_steps);

        let filename = if pkg.asset_name.is_empty() {
            format!("{}_{}.deb", pkg.name, pkg.version)
        } else {
            pkg.asset_name.clone()
        };
        let dest_file = cfg.cache_dir.join(&filename);

        download_file_verified(&pkg.download_url, &dest_file, &pkg.sha256)?;
        downloaded_paths.push((pkg.clone(), dest_file));
    }

    if download_only {
        ui::finish_progress_bar(lang);
        ui::success(
            "DOWNLOAD",
            &format!("Archives saved in {}", cfg.cache_dir.display()),
        );
        return Ok(());
    }

    let affected_names: Vec<String> = downloaded_paths
        .iter()
        .map(|(pkg, _)| pkg.name.clone())
        .collect();
    hooks::run_hooks(cfg, "pre", event_name, &affected_names)?;

    for (idx, (pkg, archive_path)) in downloaded_paths.into_iter().enumerate() {
        current_step += 1;
        ui::action_line(
            "Unpack",
            CYAN,
            idx + 1,
            &pkg.origin,
            &pkg.name,
            &pkg.version,
            &format!("type: {} • extracting", pkg.repo_type),
        );
        ui::draw_progress_bar(lang, current_step, total_steps);

        let installed_files = install_archive_to_system(cfg, &pkg, &archive_path)?;

        current_step += 1;
        ui::action_line(
            "Setup",
            GREEN,
            idx + 1,
            &pkg.origin,
            &pkg.name,
            &pkg.version,
            &format!("type: {} • installed", pkg.repo_type),
        );
        ui::draw_progress_bar(lang, current_step, total_steps);

        let is_auto = !requested_names.contains(&pkg.name);
        db.installed.retain(|i| i.name != pkg.name);
        db.installed.push(InstalledPackage {
            name: pkg.name.clone(),
            version: pkg.version.clone(),
            origin: pkg.origin.clone(),
            repo_type: pkg.repo_type.clone(),
            architecture: pkg.architecture.clone(),
            installed_path: cfg.install_root.to_string_lossy().to_string(),
            installed_size: pkg.installed_size,
            dependencies: pkg.dependencies.clone(),
            installed_files,
            auto_installed: is_auto,
        });
        cfg.save_db(&db)?;
    }

    ui::finish_progress_bar(lang);
    hooks::run_hooks(cfg, "post", event_name, &affected_names)?;
    Ok(())
}

/// `sudo pkg remove <packages...>` / `sudo pkg purge <packages...>` —
/// Original AsthelOS `pkg` UI with reverse dependency tree, disk space freed, prompt, and progress bar
pub fn cmd_remove(
    cfg: &ConfigManager,
    packages: &[String],
    purge: bool,
    yes: bool,
) -> Result<(), String> {
    let event_name = if purge { "purge" } else { "remove" };
    cmd_remove_with_event(cfg, packages, purge, yes, event_name)
}

fn cmd_remove_with_event(
    cfg: &ConfigManager,
    packages: &[String],
    purge: bool,
    yes: bool,
    event_name: &str,
) -> Result<(), String> {
    let lang = Lang::detect();
    cfg.require_root(lang)?;

    if packages.is_empty() {
        return Err("Specify at least one package to remove.".to_string());
    }

    ui::info("DEPS", lang.resolving_deps());

    let mut db = cfg.load_db();
    let sys_installed = cfg.read_system_dpkg_installed();

    let mut all_installed: HashMap<String, InstalledPackage> = sys_installed;
    for inst in &db.installed {
        all_installed.insert(inst.name.clone(), inst.clone());
    }

    let mut to_remove: Vec<InstalledPackage> = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut requested_set: HashSet<String> = HashSet::new();

    for req in packages {
        if let Some(inst) = all_installed.get(req) {
            requested_set.insert(inst.name.clone());
            collect_reverse_dependencies_to_remove(
                inst,
                &all_installed,
                &mut visited,
                &mut to_remove,
            );
        } else {
            ui::warn("REMOVE", &lang.pkg_not_installed(req));
        }
    }

    if to_remove.is_empty() {
        return Ok(());
    }

    let primary: Vec<&InstalledPackage> = to_remove
        .iter()
        .filter(|p| requested_set.contains(&p.name))
        .collect();
    let rev_deps: Vec<&InstalledPackage> = to_remove
        .iter()
        .filter(|p| !requested_set.contains(&p.name))
        .collect();

    let freed_bytes: u64 = to_remove
        .iter()
        .map(|p| {
            if p.installed_size > 0 {
                p.installed_size
            } else {
                360 * 1024
            }
        })
        .sum();

    let (_lbl_inst, _lbl_up, lbl_rem, _lbl_dl) = lang.summary_labels();
    let lbl_freed = lang.disk_impact_label(true);

    println!("\n{BOLD}{}{RESET}", lang.plan_title_remove());
    println!("  {DIM}{}{RESET}", lang.target_packages_label(primary.len()));
    for p in &primary {
        let sz = if p.installed_size > 0 {
            format_bytes(p.installed_size)
        } else {
            "360 kB".to_string()
        };
        println!(
            "    {RED}•{RESET} {BOLD}{}{RESET} {DIM}{} · {} · -{sz}{RESET}",
            p.name, p.version, p.origin
        );
    }

    if !rev_deps.is_empty() {
        println!("  {DIM}{}{RESET}", lang.reverse_deps_label(rev_deps.len()));
        for (i, r) in rev_deps.iter().enumerate() {
            let branch = if i + 1 == rev_deps.len() {
                "└─"
            } else {
                "├─"
            };
            let sz = if r.installed_size > 0 {
                format_bytes(r.installed_size)
            } else {
                "120 kB".to_string()
            };
            println!(
                "    {DIM}{branch}{RESET} {BOLD}{}{RESET} {DIM}{} · {} · -{sz}{RESET}",
                r.name, r.version, r.origin
            );
        }
    }

    println!(
        "\n  {RED}-{}{RESET} {DIM}{lbl_rem} · {lbl_freed}: -{}{RESET}\n",
        to_remove.len(),
        format_bytes(freed_bytes)
    );

    if !yes && !confirm_prompt(lang)? {
        ui::warn("ABORT", lang.abort_msg());
        return Ok(());
    }

    let total_steps = to_remove.len();
    let mut current_step = 0usize;
    ui::draw_progress_bar(lang, 0, total_steps);

    let affected_names: Vec<String> = to_remove.iter().map(|r| r.name.clone()).collect();
    hooks::run_hooks(cfg, "pre", event_name, &affected_names)?;

    for (idx, rem) in to_remove.iter().enumerate() {
        current_step += 1;
        let sz = if rem.installed_size > 0 {
            format_bytes(rem.installed_size)
        } else {
            "360 kB".to_string()
        };
        ui::action_line(
            if purge { "Purge" } else { "Remove" },
            RED,
            idx + 1,
            &rem.origin,
            &rem.name,
            &rem.version,
            &format!("type: {} • freed {sz}", rem.repo_type),
        );
        ui::draw_progress_bar(lang, current_step, total_steps);

        if rem.origin == "system/dpkg"
            || (is_root() && rem.installed_path == "/" && rem.repo_type == "deb")
        {
            let action = if purge { "--purge" } else { "--remove" };
            let status = Command::new("dpkg")
                .args([action, &rem.name])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .output()
                .map_err(|e| format!("Failed to execute dpkg: {e}"))?;

            if !status.status.success() {
                ui::clear_progress_line();
                let err_txt = String::from_utf8_lossy(&status.stderr);
                return Err(err_txt.trim().to_string());
            }
        } else {
            for f in &rem.installed_files {
                let p = Path::new(f);
                if p.exists() && p.is_file() {
                    let _ = fs::remove_file(p);
                }
            }
        }

        db.installed.retain(|i| i.name != rem.name);
        cfg.save_db(&db)?;
    }

    ui::finish_progress_bar(lang);
    hooks::run_hooks(cfg, "post", event_name, &affected_names)?;
    Ok(())
}

fn collect_reverse_dependencies_to_remove(
    target: &InstalledPackage,
    all_installed: &HashMap<String, InstalledPackage>,
    visited: &mut HashSet<String>,
    out: &mut Vec<InstalledPackage>,
) {
    if visited.contains(&target.name) {
        return;
    }
    visited.insert(target.name.clone());

    for other in all_installed.values() {
        if other.dependencies.contains(&target.name) && !visited.contains(&other.name) {
            collect_reverse_dependencies_to_remove(other, all_installed, visited, out);
        }
    }

    out.push(target.clone());
}

fn resolve_dependencies_recursive<'a>(
    pkg: &'a IndexedPackage,
    index_map: &HashMap<&'a str, &'a IndexedPackage>,
    installed: &[InstalledPackage],
    sys_installed: &HashMap<String, InstalledPackage>,
    visited: &mut HashSet<String>,
    plan: &mut Vec<IndexedPackage>,
) {
    if visited.contains(&pkg.name) {
        return;
    }
    visited.insert(pkg.name.clone());

    for dep_name in &pkg.dependencies {
        let already =
            installed.iter().any(|i| i.name == *dep_name) || sys_installed.contains_key(dep_name);
        if !already {
            if let Some(dep_pkg) = index_map.get(dep_name.as_str()) {
                resolve_dependencies_recursive(
                    dep_pkg,
                    index_map,
                    installed,
                    sys_installed,
                    visited,
                    plan,
                );
            }
        }
    }

    plan.push(pkg.clone());
}

fn download_file_conditional(url: &str, dest: &Path, timeout_secs: u32) -> bool {
    let mut args = vec![
        "-sLf".to_string(),
        "--max-time".to_string(),
        timeout_secs.to_string(),
        "-H".to_string(),
        "User-Agent: asthelos-pkg/0.1.0".to_string(),
    ];
    if dest.exists() {
        args.push("-z".to_string());
        args.push(dest.to_string_lossy().to_string());
    }
    args.push("-o".to_string());
    args.push(dest.to_string_lossy().to_string());
    args.push(url.to_string());

    Command::new("curl")
        .args(&args)
        .status()
        .map(|s| s.success() && dest.exists())
        .unwrap_or(false)
}

fn download_file_verified(url: &str, dest: &Path, expected_sha256: &str) -> Result<(), String> {
    let status = Command::new("curl")
        .args([
            "-sLf",
            "--max-time",
            "120",
            "-H",
            "User-Agent: asthelos-pkg/0.1.0",
            "-o",
            &dest.to_string_lossy(),
            url,
        ])
        .status()
        .map_err(|e| format!("Failed to run curl: {e}"))?;

    if !status.success() || !dest.exists() {
        ui::clear_progress_line();
        return Err(format!("Failed to download archive from {url}"));
    }

    if !expected_sha256.is_empty() {
        let bytes = fs::read(dest).map_err(|e| format!("Failed to read downloaded file: {e}"))?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let actual_hex = format!("{:x}", hasher.finalize());
        if !actual_hex.eq_ignore_ascii_case(expected_sha256) {
            let _ = fs::remove_file(dest);
            ui::clear_progress_line();
            return Err(format!(
                "SHA256 checksum mismatch for {}: expected {}, got {}",
                dest.display(),
                expected_sha256,
                actual_hex
            ));
        }
    }

    Ok(())
}

fn install_archive_to_system(
    cfg: &ConfigManager,
    pkg: &IndexedPackage,
    archive_path: &Path,
) -> Result<Vec<String>, String> {
    let file_str = archive_path.to_string_lossy().to_lowercase();
    let mut recorded_files = Vec::new();

    if file_str.ends_with(".deb") {
        if let Ok(out) = Command::new("dpkg-deb")
            .args(["-c", &archive_path.to_string_lossy()])
            .output()
        {
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                if let Some(path_part) = line.split_whitespace().last() {
                    let clean = path_part.trim_start_matches('.');
                    if !clean.is_empty() && clean != "/" && !clean.ends_with('/') {
                        recorded_files.push(
                            cfg.install_root
                                .join(clean.trim_start_matches('/'))
                                .to_string_lossy()
                                .to_string(),
                        );
                    }
                }
            }
        }

        if is_root() && cfg.install_root == Path::new("/") {
            let out = Command::new("dpkg")
                .args(["-i", "--force-depends", &archive_path.to_string_lossy()])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped())
                .output()
                .map_err(|e| format!("Failed to run dpkg: {e}"))?;
            if !out.status.success() {
                ui::clear_progress_line();
                return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
            }
        } else {
            let cmd = format!(
                "dpkg-deb --fsys-tarfile '{}' | tar -x --keep-directory-symlink -C '{}'",
                archive_path.display(),
                cfg.install_root.display()
            );
            let status = Command::new("sh")
                .args(["-c", &cmd])
                .status()
                .map_err(|e| format!("Failed to extract deb archive: {e}"))?;
            if !status.success() {
                ui::clear_progress_line();
                return Err(format!(
                    "dpkg-deb failed to extract {}",
                    archive_path.display()
                ));
            }
        }
        return Ok(recorded_files);
    }

    if file_str.ends_with(".tar.gz")
        || file_str.ends_with(".tgz")
        || file_str.ends_with(".tar.xz")
    {
        let stage_dir = cfg.cache_dir.join(format!("stage_{}", pkg.name));
        let _ = fs::remove_dir_all(&stage_dir);
        fs::create_dir_all(&stage_dir).map_err(|e| e.to_string())?;

        let status = Command::new("tar")
            .args([
                "-xf",
                &archive_path.to_string_lossy(),
                "-C",
                &stage_dir.to_string_lossy(),
            ])
            .status()
            .map_err(|e| format!("Failed to extract archive with tar: {e}"))?;

        if !status.success() {
            ui::clear_progress_line();
            return Err(format!("Failed to extract {}", archive_path.display()));
        }

        let bin_dir = cfg.install_root.join("usr/local/bin");
        fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;

        install_extracted_files_recursive(&stage_dir, &bin_dir, &pkg.name, &mut recorded_files)?;
        let _ = fs::remove_dir_all(&stage_dir);
        return Ok(recorded_files);
    }

    let bin_dir = cfg.install_root.join("usr/local/bin");
    fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;
    let dest_bin = bin_dir.join(&pkg.name);
    fs::copy(archive_path, &dest_bin).map_err(|e| format!("Failed to install binary: {e}"))?;
    if let Ok(meta) = fs::metadata(&dest_bin) {
        let mut perms = meta.permissions();
        perms.set_mode(0o755);
        let _ = fs::set_permissions(&dest_bin, perms);
    }
    recorded_files.push(dest_bin.to_string_lossy().to_string());
    Ok(recorded_files)
}

fn install_extracted_files_recursive(
    dir: &Path,
    bin_dir: &Path,
    pkg_name: &str,
    recorded: &mut Vec<String>,
) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            install_extracted_files_recursive(&path, bin_dir, pkg_name, recorded)?;
        } else if let Ok(meta) = entry.metadata() {
            let fname = entry.file_name().to_string_lossy().to_string();
            let mode = meta.permissions().mode();
            let is_exec = (mode & 0o111) != 0 || fname == pkg_name;
            if is_exec && !fname.ends_with(".md") && !fname.ends_with(".txt") {
                let target = bin_dir.join(&fname);
                fs::copy(&path, &target).map_err(|e| e.to_string())?;
                let mut perms = fs::metadata(&target).map_err(|e| e.to_string())?.permissions();
                perms.set_mode(0o755);
                let _ = fs::set_permissions(&target, perms);
                recorded.push(target.to_string_lossy().to_string());
            }
        }
    }
    Ok(())
}

/// `sudo pkg upgrade` / `sudo pkg full-upgrade`
pub fn cmd_upgrade(cfg: &ConfigManager, full: bool, yes: bool) -> Result<(), String> {
    let lang = Lang::detect();
    cfg.require_root(lang)?;
    let event_name = if full { "full-upgrade" } else { "upgrade" };

    ui::info("DEPS", lang.resolving_deps());

    let db = cfg.load_db();
    let index = cfg.load_index();
    let mut to_upgrade: Vec<String> = Vec::new();

    for inst in &db.installed {
        if let Some(idx_pkg) = index
            .iter()
            .find(|p| p.name == inst.name && p.origin == inst.origin)
        {
            if idx_pkg.version != inst.version {
                to_upgrade.push(inst.name.clone());
            }
        }
    }

    if to_upgrade.is_empty() {
        ui::success("UPGRADE", "All packages are up to date.");
        hooks::run_hooks(cfg, "post", event_name, &[])?;
        return Ok(());
    }

    cmd_install_with_event(cfg, &to_upgrade, yes, true, false, false, false, event_name)
}

/// `sudo pkg autoremove`
pub fn cmd_autoremove(cfg: &ConfigManager, purge: bool, yes: bool) -> Result<(), String> {
    let lang = Lang::detect();
    cfg.require_root(lang)?;

    let db = cfg.load_db();
    let orphans: Vec<String> = db
        .installed
        .iter()
        .filter(|i| i.auto_installed)
        .map(|i| i.name.clone())
        .collect();

    if orphans.is_empty() {
        ui::success("AUTOREMOVE", "No unused dependencies to remove.");
        return Ok(());
    }

    cmd_remove_with_event(cfg, &orphans, purge, yes, "autoremove")
}

/// `pkg search <query>`
pub fn cmd_search(cfg: &ConfigManager, query: &str) -> Result<(), String> {
    let db = cfg.load_db();
    let index = cfg.load_index();
    let sys_installed = cfg.read_system_dpkg_installed();
    let q = query.to_lowercase();
    let mut found = 0usize;

    let mut out = io::stdout().lock();
    for pkg in &index {
        if pkg.name.to_lowercase().contains(&q)
            || pkg.origin.to_lowercase().contains(&q)
            || pkg.description.to_lowercase().contains(&q)
        {
            let inst_badge = if db.installed.iter().any(|i| i.name == pkg.name)
                || sys_installed.contains_key(&pkg.name)
            {
                format!(" {GREEN}[installed]{RESET}")
            } else {
                String::new()
            };
            if writeln!(
                out,
                "{BOLD}{GREEN}{}{RESET} {BOLD}{CYAN}{}{RESET} {} {DIM}[type: {} • {}]{RESET}{}",
                pkg.name, pkg.origin, pkg.version, pkg.repo_type, pkg.architecture, inst_badge
            )
            .is_err()
            {
                return Ok(());
            }
            if writeln!(out, "  {}", pkg.description).is_err() {
                return Ok(());
            }
            found += 1;
        }
    }

    if found == 0 {
        ui::warn("SEARCH", &format!("No packages found matching '{query}'."));
    }
    Ok(())
}

/// `pkg show <packages...>`
pub fn cmd_show(cfg: &ConfigManager, packages: &[String]) -> Result<(), String> {
    if packages.is_empty() {
        return Err("Specify at least one package name.".to_string());
    }

    let db = cfg.load_db();
    let index = cfg.load_index();
    let sys_installed = cfg.read_system_dpkg_installed();

    for query in packages {
        if let Some(pkg) = find_package_in_index(&index, query) {
            let inst = db.installed.iter().any(|i| i.name == pkg.name)
                || sys_installed.contains_key(&pkg.name);
            println!(
                "{BOLD}{CYAN}┌─ Package: {BOLD}{GREEN}{}{RESET} {DIM}({}){RESET}",
                pkg.name, pkg.version
            );
            println!(
                "{BOLD}{CYAN}│{RESET} Origin        : {BOLD}{CYAN}{}{RESET} [{MAGENTA}type: {}{RESET}]",
                pkg.origin, pkg.repo_type
            );
            println!(
                "{BOLD}{CYAN}│{RESET} State         : {}",
                if inst {
                    format!("{GREEN}installed{RESET}")
                } else {
                    "not installed".to_string()
                }
            );
            println!("{BOLD}{CYAN}│{RESET} Architecture  : {}", pkg.architecture);
            if !pkg.maintainer.is_empty() {
                println!("{BOLD}{CYAN}│{RESET} Maintainer    : {}", pkg.maintainer);
            }
            if pkg.size > 0 {
                println!(
                    "{BOLD}{CYAN}│{RESET} Download Size : {}",
                    format_bytes(pkg.size)
                );
            }
            if pkg.installed_size > 0 {
                println!(
                    "{BOLD}{CYAN}│{RESET} Installed Size: {}",
                    format_bytes(pkg.installed_size)
                );
            }
            if !pkg.dependencies.is_empty() {
                println!(
                    "{BOLD}{CYAN}│{RESET} Dependencies  : {}",
                    pkg.dependencies.join(", ")
                );
            }
            println!("{BOLD}{CYAN}│{RESET} Download URL  : {}", pkg.download_url);
            println!("{BOLD}{CYAN}└─{RESET} Description  : {}\n", pkg.description);
        } else if let Some(sys_pkg) = sys_installed.get(query) {
            println!(
                "{BOLD}{CYAN}┌─ Package: {BOLD}{GREEN}{}{RESET} {DIM}({}){RESET}",
                sys_pkg.name, sys_pkg.version
            );
            println!("{BOLD}{CYAN}│{RESET} Origin        : {CYAN}system/dpkg{RESET}");
            println!("{BOLD}{CYAN}│{RESET} State         : {GREEN}installed{RESET}");
            println!(
                "{BOLD}{CYAN}│{RESET} Architecture  : {}",
                sys_pkg.architecture
            );
            if sys_pkg.installed_size > 0 {
                println!(
                    "{BOLD}{CYAN}│{RESET} Installed Size: {}",
                    format_bytes(sys_pkg.installed_size)
                );
            }
            if !sys_pkg.dependencies.is_empty() {
                println!(
                    "{BOLD}{CYAN}└─{RESET} Dependencies  : {}\n",
                    sys_pkg.dependencies.join(", ")
                );
            } else {
                println!("{BOLD}{CYAN}└─{RESET}");
            }
        } else {
            ui::warn("SHOW", &format!("Package '{query}' not found."));
        }
    }
    Ok(())
}

/// `pkg list`
pub fn cmd_list(
    cfg: &ConfigManager,
    installed: bool,
    upgradable: bool,
    filter: Option<&str>,
) -> Result<(), String> {
    let db = cfg.load_db();
    let index = cfg.load_index();
    let sys_installed = cfg.read_system_dpkg_installed();
    let mut out = io::stdout().lock();

    if installed {
        for inst in &db.installed {
            if let Some(f) = filter {
                if !inst.name.contains(f) && !inst.origin.contains(f) {
                    continue;
                }
            }
            let _ = writeln!(
                out,
                "{BOLD}{GREEN}{}{RESET} {BOLD}{CYAN}{}{RESET} {} {DIM}[type: {} • {}]{RESET} {GREEN}[installed]{RESET}",
                inst.name, inst.origin, inst.version, inst.repo_type, inst.architecture
            );
        }
        for sys in sys_installed.values() {
            if let Some(f) = filter {
                if !sys.name.contains(f) {
                    continue;
                }
            }
            let _ = writeln!(
                out,
                "{BOLD}{GREEN}{}{RESET} {BOLD}{CYAN}{}{RESET} {} {DIM}[{}]{RESET} {GREEN}[installed]{RESET}",
                sys.name, sys.origin, sys.version, sys.architecture
            );
        }
        return Ok(());
    }

    for pkg in &index {
        if let Some(f) = filter {
            if !pkg.name.contains(f) && !pkg.origin.contains(f) {
                continue;
            }
        }
        let inst = db
            .installed
            .iter()
            .find(|i| i.name == pkg.name)
            .or_else(|| sys_installed.get(&pkg.name));
        if upgradable {
            match inst {
                Some(i) if i.version != pkg.version => {}
                _ => continue,
            }
        }
        let badge = if inst.is_some() {
            format!(" {GREEN}[installed]{RESET}")
        } else {
            String::new()
        };
        if writeln!(
            out,
            "{BOLD}{GREEN}{}{RESET} {BOLD}{CYAN}{}{RESET} {} {DIM}[type: {} • {}]{RESET}{}",
            pkg.name, pkg.origin, pkg.version, pkg.repo_type, pkg.architecture, badge
        )
        .is_err()
        {
            break;
        }
    }

    Ok(())
}

/// `pkg depends` / `pkg rdepends`
pub fn cmd_depends(cfg: &ConfigManager, packages: &[String], reverse: bool) -> Result<(), String> {
    let index = cfg.load_index();
    for query in packages {
        if let Some(pkg) = find_package_in_index(&index, query) {
            println!(
                "{BOLD}{GREEN}{}{RESET} {BOLD}{CYAN}{}{RESET} {DIM}[type: {}]{RESET}",
                pkg.name, pkg.origin, pkg.repo_type
            );
            if reverse {
                for other in &index {
                    if other.dependencies.contains(&pkg.name) {
                        println!("  ├─ Reverse Depends: {BOLD}{}{RESET}", other.name);
                    }
                }
            } else if pkg.dependencies.is_empty() {
                println!("  └─ (no dependencies)");
            } else {
                for (i, dep) in pkg.dependencies.iter().enumerate() {
                    let br = if i + 1 == pkg.dependencies.len() {
                        "└─"
                    } else {
                        "├─"
                    };
                    println!("  {br} Depends: {BOLD}{dep}{RESET}");
                }
            }
        } else {
            ui::warn("DEPENDS", &format!("Package '{query}' not found."));
        }
    }
    Ok(())
}

/// `pkg policy`
pub fn cmd_policy(cfg: &ConfigManager, packages: &[String]) -> Result<(), String> {
    let db = cfg.load_db();
    let index = cfg.load_index();
    let sys_installed = cfg.read_system_dpkg_installed();
    for query in packages {
        if let Some(pkg) = find_package_in_index(&index, query) {
            let inst_ver = db
                .installed
                .iter()
                .find(|i| i.name == pkg.name)
                .map(|i| i.version.as_str())
                .or_else(|| sys_installed.get(&pkg.name).map(|i| i.version.as_str()))
                .unwrap_or("(none)");
            println!("{BOLD}{GREEN}{}{RESET}:", pkg.name);
            println!("  ├─ Installed : {inst_ver}");
            println!("  ├─ Candidate : {}", pkg.version);
            println!(
                "  └─ Origin    : {BOLD}{CYAN}{}{RESET} {DIM}[type: {} • {}]{RESET}",
                pkg.origin, pkg.repo_type, pkg.download_url
            );
        } else {
            ui::warn("POLICY", &format!("Unable to locate package '{query}'."));
        }
    }
    Ok(())
}

/// `sudo pkg clean` / `sudo pkg autoclean`
pub fn cmd_clean(cfg: &ConfigManager) -> Result<(), String> {
    let lang = Lang::detect();
    cfg.require_root(lang)?;

    if cfg.cache_dir.exists() {
        let _ = fs::remove_dir_all(&cfg.cache_dir);
        let _ = fs::create_dir_all(&cfg.cache_dir);
    }
    ui::success("CLEAN", "Cleared package and release cache.");
    Ok(())
}

/// `pkg check`
pub fn cmd_check(cfg: &ConfigManager) -> Result<(), String> {
    let db = cfg.load_db();
    let index = cfg.load_index();
    ui::success(
        "CHECK",
        &format!(
            "{} sources configured • {} packages indexed",
            db.sources.len(),
            index.len()
        ),
    );
    Ok(())
}

/// `pkg edit-sources`
pub fn cmd_edit_sources(cfg: &ConfigManager) -> Result<(), String> {
    let db = cfg.load_db();
    println!("{BOLD}Configured pkg sources:{RESET}");
    println!("  State directory: {}", cfg.base_dir.display());
    println!("  Database file  : {}", cfg.db_file.display());
    println!("  Index file     : {}", cfg.index_file.display());
    for src in &db.sources {
        println!(
            "  • {BOLD}{CYAN}{}{RESET} ({}) -> {}",
            src.namespace, src.origin_kind, src.location
        );
    }
    Ok(())
}

fn find_package_in_index(index: &[IndexedPackage], query: &str) -> Option<IndexedPackage> {
    index
        .iter()
        .find(|p| {
            p.name == query
                || p.origin == query
                || format!("{}/{}", p.origin, p.name) == query
        })
        .cloned()
}

fn detect_system_arch() -> String {
    if let Ok(override_arch) = std::env::var("PKG_ARCH") {
        let norm = override_arch.trim().to_lowercase();
        if !norm.is_empty() {
            return match norm.as_str() {
                "x86_64" | "x64" => "amd64".to_string(),
                "aarch64" => "arm64".to_string(),
                "armv7l" | "arm" => "armhf".to_string(),
                _ => norm,
            };
        }
    }
    match std::env::consts::ARCH {
        "x86_64" => "amd64".to_string(),
        "aarch64" => "arm64".to_string(),
        "arm" => "armhf".to_string(),
        "powerpc64" => "ppc64el".to_string(),
        "riscv64" => "riscv64".to_string(),
        "s390x" => "s390x".to_string(),
        "x86" => "i386".to_string(),
        other => other.to_string(),
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let b = bytes as f64;
    if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} kB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

fn confirm_prompt(lang: Lang) -> Result<bool, String> {
    print!(
        "{BOLD}{YELLOW}[PKG:CONFIRM]{RESET} {}",
        lang.confirm_prompt()
    );
    let _ = io::stdout().flush();
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|e| format!("Failed to read input: {e}"))?;
    Ok(lang.is_affirmative(&input))
}
