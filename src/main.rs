mod cli;
mod config;
mod hooks;
mod i18n;
mod manager;
mod models;
mod repo;
mod ui;

use clap::Parser;
use cli::{Cli, Commands, RepoAction};
use config::ConfigManager;
use std::path::Path;
use std::process;

fn main() {
    let parsed = Cli::parse();
    let cfg = ConfigManager::new();

    let result = match parsed.command {
        Commands::Add(args) => match args.first.as_str() {
            "github" | "git" => {
                if let Some(target) = args.second {
                    repo::add_github(&cfg, &target)
                } else {
                    Err("Usage: pkg add github <owner/repo>".to_string())
                }
            }
            "ppa" => {
                if let Some(target) = args.second {
                    repo::add_ppa(&cfg, &target, args.suite.as_deref())
                } else {
                    Err("Usage: pkg add ppa <owner/ppa>".to_string())
                }
            }
            "deb" => {
                if let Some(mirror) = args.second {
                    repo::add_deb_mirror(
                        &cfg,
                        &mirror,
                        args.suite.as_deref(),
                        &args.components,
                        args.name.as_deref(),
                    )
                } else {
                    Err("Usage: pkg add deb <mirror>".to_string())
                }
            }
            mirror => {
                // `pkg add <mirror>` works identically to `pkg add deb <mirror>`
                if mirror.starts_with("ppa:") {
                    repo::add_ppa(&cfg, mirror, args.suite.as_deref())
                } else {
                    repo::add_deb_mirror(
                        &cfg,
                        mirror,
                        args.suite.as_deref(),
                        &args.components,
                        args.name.as_deref(),
                    )
                }
            }
        },
        Commands::Repo { action } => match action {
            RepoAction::List => {
                repo::list_repos(&cfg);
                Ok(())
            }
            RepoAction::Remove { target } => repo::remove_repo(&cfg, &target),
            RepoAction::Init { dir, name } => repo::init_master(Path::new(&dir), &name),
        },
        Commands::Update => manager::cmd_update(&cfg),
        Commands::Install {
            packages,
            yes,
            reinstall,
            no_recommends,
            fix_broken,
            download_only,
        } => manager::cmd_install(
            &cfg,
            &packages,
            yes,
            reinstall,
            no_recommends,
            fix_broken,
            download_only,
        ),
        Commands::Remove { packages, yes } => manager::cmd_remove(&cfg, &packages, false, yes),
        Commands::Purge { packages, yes } => manager::cmd_remove(&cfg, &packages, true, yes),
        Commands::Upgrade { yes } => manager::cmd_upgrade(&cfg, false, yes),
        Commands::FullUpgrade { yes } => manager::cmd_upgrade(&cfg, true, yes),
        Commands::Autoremove { purge, yes } => manager::cmd_autoremove(&cfg, purge, yes),
        Commands::Search { query } => manager::cmd_search(&cfg, &query),
        Commands::Show { packages } => manager::cmd_show(&cfg, &packages),
        Commands::List {
            installed,
            upgradable,
            filter,
        } => manager::cmd_list(&cfg, installed, upgradable, filter.as_deref()),
        Commands::Depends { packages } => manager::cmd_depends(&cfg, &packages, false),
        Commands::Rdepends { packages } => manager::cmd_depends(&cfg, &packages, true),
        Commands::Policy { packages } => manager::cmd_policy(&cfg, &packages),
        Commands::Clean | Commands::Autoclean => manager::cmd_clean(&cfg),
        Commands::Check => manager::cmd_check(&cfg),
        Commands::Download { packages } | Commands::Source { packages } => {
            manager::cmd_install(&cfg, &packages, true, true, false, false, true)
        }
        Commands::EditSources => manager::cmd_edit_sources(&cfg),
    };

    if let Err(err) = result {
        ui::error(&err);
        process::exit(100);
    }
}
