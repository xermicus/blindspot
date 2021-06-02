use std::io;

use anyhow::Context;
use structopt::clap::Shell;
use structopt::StructOpt;

use crate::bspm::{self, *};

#[derive(StructOpt, Debug)]
#[structopt(about = "The blindspot package manager")]
pub enum Command {
    #[structopt(
        name = "init",
        about = "Create a fresh config file and automatically install the blindspot package manager binary"
    )]
    Init {
        #[structopt(long, short, help = "Do not install blindspot automatically")]
        no_install: bool,
    },
    #[structopt(
        name = "install",
        about = "Install a single binary application from a download static URL or github repo"
    )]
    Install {
        #[structopt(help = "Name of the package. This can be anything you want.")]
        name: String,
        #[structopt(
            help = "Either a direct http download URL (the URL should not change over time and always provide the latest version) or a github repo in the form of `username/repository`."
        )]
        url: String,
        #[structopt(short, long, help = "Install anyways and overwrite existing versions")]
        force: bool,
        #[structopt(help = "Set compression", short, long, possible_values = &bspm::installer::Compression::variants(), case_insensitive = false)]
        compression: Option<bspm::installer::Compression>,
        #[structopt(help = "Set archive type", short, long, possible_values = &bspm::installer::Archived::variants(), case_insensitive = false)]
        archive: Option<bspm::installer::Archived>,
    },
    #[structopt(
        name = "remove",
        about = "Remove a package",
        alias = "uninstall",
        alias = "delete"
    )]
    Remove {
        #[structopt(help = "Name of the package")]
        name: String,
    },
    #[structopt(name = "revert", about = "Revert the last update of a package")]
    Revert {
        #[structopt(help = "Name of the package (only works once after every update)")]
        name: String,
    },
    #[structopt(name = "update", about = "Update installed packages")]
    Update {
        #[structopt(help = "List of packages to update")]
        packages: Vec<String>,
    },
    #[structopt(name = "list", about = "List currently installed packages")]
    List {
        #[structopt(short, long)]
        debug: bool,
    },
    #[structopt(name = "completion", about = "Generate bash completion")]
    Completion {
        #[structopt(short, long, default_value = "bash", possible_values = &Shell::variants())]
        shell: Shell,
    },
}

impl Command {
    pub async fn go(&self) -> anyhow::Result<()> {
        let bspm = Bspm::new()
            .await
            .context("BSPM failed to start\nTry `bspm init` if you are running it the first time");
        match self {
            Command::Init { no_install } => {
                Bspm::default().create_config().await?;
                if !no_install {
                    Bspm::new()
                        .await?
                        .install(
                            "blindspot".to_string(),
                            "xermicus/blindspot".to_string(),
                            false,
                            None,
                            None,
                        )
                        .await?;
                }
                ui::context("🎉", "blindspot")
                    .await
                    .notify("Initialization successful")
                    .await;
                ui::context("🐚", "blindspot").await
                    .notify("Run `blindspot completion --help` to see if completion for your shell is available" ).await;
            }
            Command::Install {
                name,
                url,
                force,
                compression,
                archive,
            } => {
                bspm?
                    .install(
                        name.clone(),
                        url.clone(),
                        *force,
                        compression.clone(),
                        archive.clone(),
                    )
                    .await?;
            }
            Command::Remove { name } => {
                bspm?.delete(name).await?;
            }
            Command::Revert { name } => {
                bspm?.revert(name).await?;
            }
            Command::Update { packages } => {
                bspm?.update(packages.to_vec()).await?;
            }
            Command::List { debug } => {
                if *debug {
                    dbg!(bspm?);
                } else {
                    bspm?.list();
                }
                return Ok(());
            }
            Command::Completion { shell } => {
                let stdout = io::stdout();
                let mut handle = stdout.lock();
                Command::clap().gen_completions_to(env!("CARGO_PKG_NAME"), *shell, &mut handle);
                return Ok(());
            }
        }
        ui::context("", "").await.quit().await?;
        Ok(())
    }
}
