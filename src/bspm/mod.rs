use std::{
    env,
    path::{Path, PathBuf},
};

use anyhow::Context;
use async_std::fs::{create_dir, File};
use async_std::prelude::*;

mod package;
use package::Package;

pub mod installer;
use installer::Installer;

pub mod ui;
use ui::context;

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct BSPM {
    packages: Vec<Package>,
}

impl BSPM {
    pub async fn new() -> anyhow::Result<BSPM> {
        let path = cfg_path().await;
        let mut buffer = String::new();
        File::open(&path)
            .await
            .context(format!(
                "Config file not found (overwrite using the BSPM_CONFIG env var): {}",
                &path.display()
            ))?
            .read_to_string(&mut buffer)
            .await
            .context(format!("Failed to read config file: {}", path.display()))?;
        Ok(serde_yaml::from_str(&buffer)
            .context(format!("Invalid BSPM config file: {}", path.display()))?)
    }

    pub async fn create_config(&self) -> anyhow::Result<()> {
        let path = cfg_path().await;
        if Path::new(&path).exists() {
            context("🚧", "blindspot")
                .await
                .notify(&format!(
                    "Config file {} already exists, not overwriting",
                    &path.display()
                ))
                .await;
            return Ok(());
        }
        self.write_config().await
    }

    pub async fn write_config(&self) -> anyhow::Result<()> {
        let path = cfg_path().await;
        let y = serde_yaml::to_string(self)?;
        File::create(&path)
            .await
            .context(format!("Can not create file: {}", &path.display()))?
            .write_all(y.as_bytes())
            .await
            .context(format!("Can not write to file: {}", &path.display()))?;
        Ok(())
    }

    pub async fn install(
        &mut self,
        name: String,
        url: String,
        force: bool,
        compression: Option<installer::Compression>,
        archive: Option<installer::Archived>,
    ) -> anyhow::Result<()> {
        let ctx = context("🔨", &name).await;
        ctx.notify("Building package").await;
        let mut path = bin_path().await;
        path.push(&name);
        let mut pkg = Package {
            name: name.clone(),
            installer: Installer {
                url,
                path,
                compression,
                archive,
                backup: None,
            },
            release: None,
            last_update: None,
            github: None,
        };
        if self.packages.contains(&pkg) {
            let ctx = context("❌", &name).await;
            ctx.notify(&format!("Package is already installed: `{}`", &pkg))
                .await;
            if !force && ctx.ask("Enter `y` to force installation").await? != "y" {
                return Ok(());
            }
            context("🤷", &name)
                .await
                .notify("Installing anyways")
                .await;
            self.packages.retain(|x| x != &pkg);
        }
        pkg.install().await?;

        self.packages.push(pkg);
        self.packages.dedup();
        ctx.notify("Package is installed").await;
        self.write_config()
            .await
            .context("failed to save config file")
    }

    pub async fn delete(&mut self, name: &str) -> anyhow::Result<()> {
        let ctx = context("🪦 ", name).await;
        ctx.notify("Deleting package").await;
        for (i, pkg) in self.packages.iter().enumerate() {
            if pkg.name != name {
                continue;
            }
            pkg.installer.uninstall(&ctx).await?;
            self.packages.remove(i);
            ctx.notify("Package is deleted and removed from disk").await;
            return self
                .write_config()
                .await
                .context("failed to save config file");
        }
        ctx.notify("This package is not installed").await;
        Ok(())
    }

    pub async fn revert(&mut self, name: &str) -> anyhow::Result<()> {
        let ctx = context("⏪", name).await;
        ctx.notify("Reverting package").await;
        for pkg in self.packages.iter_mut() {
            if pkg.name != name {
                continue;
            }
            pkg.installer.revert(&ctx).await?;
            return self
                .write_config()
                .await
                .context("failed to save config file");
        }
        ctx.notify("This package is not installed").await;
        Ok(())
    }

    pub async fn update(&mut self, packages: Vec<String>) -> anyhow::Result<()> {
        if self.packages.is_empty() {
            context("🏜 ", "blindspot")
                .await
                .notify("This is no mans land")
                .await;
            return Ok(());
        }
        let mut handles = Vec::new();
        for pkg in self.packages.iter() {
            if !packages.contains(&pkg.name) && !packages.is_empty() {
                continue;
            }
            let pkg = pkg.clone();
            handles.push(std::thread::spawn(move || {
                smol::run(async move {
                    let result = pkg.update().await;
                    let ctx = context("❌", &pkg.name).await;
                    if result.is_err() {
                        ctx.notify(&format!("Update failed: {:?}", &result).replace("\n", "."))
                            .await;
                    }
                    ctx.quit().await.expect("UI failure");
                    result
                })
            }));
        }
        for handle in handles {
            let updated = match handle.join().expect("Thread join failure") {
                Ok(pkg) => pkg,
                Err(_) => continue,
            };
            for (i, pkg) in self.packages.iter().enumerate() {
                if *pkg == updated {
                    self.packages.remove(i);
                    self.packages.push(updated);
                    break;
                }
            }
        }
        self.write_config()
            .await
            .context("failed to save config file")
    }

    pub fn list(&self) {
        for pkg in &self.packages {
            println!(
                "{}{}{} {}",
                termion::style::Bold,
                pkg.name,
                termion::style::Reset,
                pkg.github.as_ref().unwrap_or(&pkg.installer.url),
            )
        }
    }
}

pub async fn cfg_path() -> PathBuf {
    if let Ok(v) = env::var("BSPM_CONFIG") {
        return PathBuf::from(v);
    }
    let mut result = dirs_next::config_dir().expect(
        "Unable to find your config dir.
                 Please specify a config file manually using the `BSPM_CONFIG` env var.",
    );
    result.push("blindspot");
    if !result.exists() {
        create_dir(&result)
            .await
            .unwrap_or_else(|_| panic!("Failed to create config dir: {}", &result.display()))
    }
    result.push("bspm.yaml");
    result
}

pub async fn bin_path() -> PathBuf {
    if let Ok(v) = env::var("BSPM_BIN_DIR") {
        return PathBuf::from(v);
    }
    let result = dirs_next::executable_dir().expect(
        "Unable to find you bin dir.
                 Please specify a binary dir manually using the `BSPM_BIN_DIR` env var.",
    );
    if !result.exists() {
        create_dir(&result)
            .await
            .unwrap_or_else(|_| panic!("Failed to create bin dir: {}", &result.display()))
    }
    result
}

pub async fn data_path() -> PathBuf {
    if let Ok(v) = env::var("BSPM_DATA_DIR") {
        return PathBuf::from(v);
    }
    let mut result = dirs_next::data_dir().expect(
        "Unable to find you bin dir.
                 Please specify a binary dir manually using the `BSPM_DATA_DIR` env var.",
    );
    if !result.exists() {
        create_dir(&result)
            .await
            .unwrap_or_else(|_| panic!("Failed to create data dir: {}", &result.display()))
    }
    result.push("blindspot");
    if !result.exists() {
        create_dir(&result)
            .await
            .unwrap_or_else(|_| panic!("Failed to create data dir: {}", &result.display()))
    }
    result
}
