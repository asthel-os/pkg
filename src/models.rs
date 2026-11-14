use serde::{Deserialize, Serialize};

/// Simple JSON manifest for a master repository (e.g., pkg-master/repository.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MasterManifest {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub repos: Vec<RepoEntry>,
}

/// Individual repository entry inside the JSON manifest or added via CLI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub repo_type: String, // "deb", "git" (or "github"), "ppa"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suite: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub components: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>, // e.g., "myuser/myrepo" or "ppa:user/name"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>, // e.g., "asthel-os/packages.json"
}

impl RepoEntry {
    pub fn full_name(&self, parent_namespace: &str) -> String {
        if parent_namespace.is_empty() || self.name.contains('/') {
            self.name.clone()
        } else {
            format!("{}/{}", parent_namespace, self.name)
        }
    }

    pub fn normalized_type(&self) -> &str {
        match self.repo_type.as_str() {
            "github" | "git" => "git",
            "ppa" => "ppa",
            _ => "deb",
        }
    }
}

/// Manifest inside `<channel>/packages.json` (e.g., `pkg-master/asthel-os/packages.json`)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitPackagesManifest {
    pub packages: Vec<GitPackageDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitPackageDef {
    pub name: String,
    pub repo: String, // e.g., "asthel-os/pkg"
}

/// Repository source registered in pkg state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredSource {
    pub namespace: String,
    #[serde(default)]
    pub slug: String,        // e.g., "asthel-os/pkg-master"
    pub origin_kind: String, // "master-json", "git", "ppa", "deb"
    pub location: String,    // URL to repository.json or local path
    pub repos: Vec<RepoEntry>,
}

/// Package indexed in pkg's fast TSV index (`/var/lib/pkg/lists/index.tsv`)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedPackage {
    pub name: String,
    pub version: String,
    pub origin: String,    // e.g., "asthel-os/asthel-os" or "asthel-os/ubuntu"
    pub repo_type: String, // "git", "deb", "ppa"
    pub architecture: String,
    pub description: String,
    pub download_url: String,
    #[serde(default)]
    pub asset_name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub installed_size: u64,
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub maintainer: String,
    #[serde(default)]
    pub section: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// Package installed on the system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPackage {
    pub name: String,
    pub version: String,
    pub origin: String,
    pub repo_type: String,
    pub architecture: String,
    pub installed_path: String,
    #[serde(default)]
    pub installed_size: u64,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub installed_files: Vec<String>,
    #[serde(default)]
    pub auto_installed: bool,
}

/// Lightweight persistent database (`db.json` stores only sources and pkg-installed packages)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PkgDatabase {
    pub sources: Vec<RegisteredSource>,
    pub installed: Vec<InstalledPackage>,
}
