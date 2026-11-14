# `pkg` — AsthelOS Package Manager

`pkg` is the official package manager for **AsthelOS**, written in Rust. It supports standard `.deb` mirrors, Launchpad `ppa` repositories, and direct `.deb` package installation from **GitHub Releases**, unified under clean repository namespaces (e.g., `asthel-os/ubuntu`, `asthel-os/pkg`, `asthel-os/drivers`).

---

## Features

- **Unified Repository Namespaces**: Maps upstream DEB mirrors, PPAs, and GitHub Release repositories under `<namespace>/<repo>` via a simple `repository.json` manifest (such as `asthel-os/pkg-master`).
- **GitHub Releases Integration (`type: "git"`)**: Downloads the latest `.deb` package for the system architecture directly from GitHub Releases without cloning or compiling source code.
- **Fast Parallel Sync (`sudo pkg update`)**: Verifies master manifests live, fetches repository indexes in parallel threads, and stores a high-speed columnar package index (`/var/lib/pkg/lists/index.tsv`).
- **Full Dependency Resolution & Checksum Verification**: Resolves recursive dependencies, calculates download and disk usage, verifies **SHA256** digests, and integrates with `/var/lib/dpkg/status`.
- **Original AsthelOS TUI**: Tree-structured installation/removal plans, interactive confirmation prompts, and a live bottom progress bar.
- **Multi-Language Support (i18n)**: Automatically adapts to the system locale (`LANG` / `LC_ALL`), supporting English, Portuguese, Spanish, French, German, Italian, Russian, Japanese, and Chinese.

---

## Building from Source

```bash
cargo build --release
sudo install -Dm755 target/release/pkg /usr/local/bin/pkg
```

---

## Adding Repositories

`pkg` starts with no default repositories pre-configured. You can add a master repository (containing a `repository.json`) or individual repositories:

```bash
# Add the AsthelOS master repository from GitHub
sudo pkg add github asthel-os/pkg-master
# or via full URL:
sudo pkg add github https://github.com/asthel-os/pkg-master.git

# Add a single GitHub Releases repository
sudo pkg add github owner/repo

# Add a Launchpad PPA
sudo pkg add ppa graphics-drivers/ppa

# Add a DEB mirror (both commands are identical)
sudo pkg add deb http://archive.ubuntu.com/ubuntu
sudo pkg add http://archive.ubuntu.com/ubuntu
```

---

## Usage

```bash
# Synchronize all configured repositories
sudo pkg update

# Search for packages
pkg search neofetch

# Display detailed package information
pkg show neofetch

# Install packages (lists target packages, required dependencies, and disk impact)
sudo pkg install neofetch

# Remove or purge installed packages
sudo pkg remove neofetch
sudo pkg purge neofetch

# Upgrade installed packages
sudo pkg upgrade
sudo pkg full-upgrade

# Remove unused dependencies
sudo pkg autoremove

# List repositories or remove a repository
pkg repo list
sudo pkg repo remove asthel-os/pkg-master
```

---

## Master Repository Structure (`pkg-master`)

`repository.json`:
```json
{
  "name": "asthel-os",
  "repos": [
    {
      "name": "ubuntu",
      "type": "deb",
      "url": "http://archive.ubuntu.com/ubuntu",
      "suite": "resolute",
      "components": ["main", "restricted", "universe", "multiverse"]
    },
    {
      "name": "asthel-os",
      "type": "git",
      "path": "asthel-os/packages.json"
    },
    {
      "name": "drivers",
      "type": "ppa",
      "repo": "ppa:graphics-drivers/ppa"
    }
  ]
}
```

`asthel-os/packages.json`:
```json
{
  "packages": [
    {
      "name": "pkg",
      "repo": "asthel-os/pkg"
    }
  ]
}
```
