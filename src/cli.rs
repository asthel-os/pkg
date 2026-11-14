use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "pkg",
    version = "0.1.0",
    about = "AsthelOS Package Manager (GitHub Releases, PPA, and DEB support)"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Add a repository:
    /// - pkg add github <owner/repo>
    /// - pkg add ppa <owner/ppa>
    /// - pkg add deb <mirror>
    /// - pkg add <mirror> (same as pkg add deb <mirror>)
    Add(AddArgs),

    /// Manage registered repositories (list, remove, init)
    Repo {
        #[command(subcommand)]
        action: RepoAction,
    },

    /// Synchronize package indexes from configured repositories (GitHub Releases, PPA, and DEB)
    Update,

    /// Install packages (from GitHub Releases, PPA, or DEB mirrors)
    Install {
        packages: Vec<String>,
        #[arg(short = 'y', long = "yes")]
        yes: bool,
        #[arg(long = "reinstall")]
        reinstall: bool,
        #[arg(long = "no-install-recommends")]
        no_recommends: bool,
        #[arg(short = 'f', long = "fix-broken")]
        fix_broken: bool,
        #[arg(short = 'd', long = "download-only")]
        download_only: bool,
    },

    /// Remove installed packages
    Remove {
        packages: Vec<String>,
        #[arg(short = 'y', long = "yes")]
        yes: bool,
    },

    /// Remove packages and purge their configuration files
    Purge {
        packages: Vec<String>,
        #[arg(short = 'y', long = "yes")]
        yes: bool,
    },

    /// Upgrade installed packages to their latest available version/release
    Upgrade {
        #[arg(short = 'y', long = "yes")]
        yes: bool,
    },

    /// Perform a full system upgrade
    #[command(visible_alias = "dist-upgrade")]
    FullUpgrade {
        #[arg(short = 'y', long = "yes")]
        yes: bool,
    },

    /// Remove automatically installed packages that are no longer needed
    Autoremove {
        #[arg(long = "purge")]
        purge: bool,
        #[arg(short = 'y', long = "yes")]
        yes: bool,
    },

    /// Search the package index
    Search {
        query: String,
    },

    /// Show detailed information about one or more packages
    #[command(visible_alias = "info")]
    Show {
        packages: Vec<String>,
    },

    /// List packages from the index or installed database
    List {
        #[arg(short = 'i', long = "installed")]
        installed: bool,
        #[arg(short = 'u', long = "upgradable")]
        upgradable: bool,
        filter: Option<String>,
    },

    /// Show dependencies for a package
    Depends {
        packages: Vec<String>,
    },

    /// Show reverse dependencies for a package
    Rdepends {
        packages: Vec<String>,
    },

    /// Show package origin and version table
    Policy {
        packages: Vec<String>,
    },

    /// Clear the local cache of downloaded package files
    Clean,

    /// Clear obsolete package files from cache
    Autoclean,

    /// Verify package database integrity
    Check,

    /// Download the binary package/release archive into cache without installing
    Download {
        packages: Vec<String>,
    },

    /// Download package archive
    Source {
        packages: Vec<String>,
    },

    /// Display configured repository sources
    EditSources,
}

#[derive(Args, Debug)]
pub struct AddArgs {
    /// Can be:
    /// - `github <owner/repo>` (or `git <owner/repo>`)
    /// - `ppa <owner/ppa>`
    /// - `deb <mirror>`
    /// - `<mirror>` directly (identical to `deb <mirror>`)
    pub first: String,

    /// Target repository or mirror URL when the first argument is `github`, `ppa`, or `deb`
    pub second: Option<String>,

    /// Optional suite for deb/ppa repositories (defaults to detected OS codename)
    #[arg(short = 's', long = "suite")]
    pub suite: Option<String>,

    /// Optional custom name for the deb mirror
    #[arg(short = 'n', long = "name")]
    pub name: Option<String>,

    /// Optional comma-separated components for deb mirrors (default: main)
    #[arg(short = 'c', long = "components", value_delimiter = ',')]
    pub components: Vec<String>,
}

#[derive(Subcommand, Debug)]
pub enum RepoAction {
    /// List all registered repositories
    List,

    /// Remove a registered repository
    Remove {
        target: String,
    },

    /// Initialize a new pkg-master template directory with repository.json
    Init {
        #[arg(default_value = "./pkg-master")]
        dir: String,
        #[arg(short = 'n', long = "name", default_value = "asthel-os")]
        name: String,
    },
}
