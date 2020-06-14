use anyhow::{anyhow, bail, Context};
use chrono::prelude::*;
use isahc::prelude::*;
use serde_json::Value;
use std::str::FromStr;

use super::{installer::Installer, ui::context};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Package {
    pub name: String,
    pub installer: Installer,
    pub release: Option<Release>,
    pub last_update: Option<DateTime<Utc>>,
    pub github: Option<String>,
}

impl Package {
    pub async fn install(&mut self) -> anyhow::Result<()> {
        let ctx = context("📦", &self.name).await;
        if self.installer.url.split('/').count() != 2 {
            self.release = Some(Release::Dated(Utc::now()));
            return self.installer.install(&ctx).await;
        }
        let github_release: Value = self
            .github_get_latest_release(&self.installer.url)
            .await
            .context("Failed to resolve GitHub repository")?;
        self.github = Some(self.installer.url.clone());
        self.installer.url = self.github_dl_url(&github_release).await?;
        self.release = Some(Release::Version(self.github_tag_name(&github_release)?));
        self.installer.install(&ctx).await
    }

    pub async fn update(&self) -> anyhow::Result<Package> {
        let mut pkg = self.clone();
        let ctx = context("⛽", &self.name).await;
        ctx.notify("Updating package").await;
        ctx.notify(&format!("Last update: {:?}", self.last_update))
            .await;
        if pkg.github.is_none() {
            pkg.install().await?;
            pkg.last_update = Some(Utc::now());
            return Ok(pkg);
        }
        let installed_release = match &pkg.release {
            Some(Release::Version(current)) => current.clone(),
            _ => bail!("Corrupted package (please reinstall)"),
        };
        let github_release: Value = pkg
            .github_get_latest_release(pkg.github.as_ref().unwrap())
            .await
            .context("Failed to resolve GitHub repository")?;
        let latest_release = pkg.github_tag_name(&github_release)?;
        ctx.notify(&format!("Installed release: {}", installed_release))
            .await;
        ctx.notify(&format!("Latest release: {}", &latest_release))
            .await;
        if latest_release == installed_release {
            ctx.notify("Looks like the latest release is already installed")
                .await;
            return Ok(pkg);
        }
        ctx.notify(&format!("Other release available: {}", latest_release))
            .await;
        pkg.installer.url = pkg.github_dl_url(&github_release).await?;
        pkg.install().await?;
        pkg.release = Some(Release::Version(latest_release));
        pkg.last_update = Some(Utc::now());
        Ok(pkg)
    }

    fn github_tag_name(&self, release: &Value) -> anyhow::Result<String> {
        let result;
        match &release["tag_name"] {
            Value::String(v) => result = v.to_string(),
            _ => bail!("No browser download url in asset"),
        }
        Ok(result)
    }

    async fn github_dl_url(&self, release: &Value) -> anyhow::Result<String> {
        let ctx = context("🪐", &self.name).await;
        let assets = match &release["assets"] {
            Value::Array(v) => v,
            _ => bail!("No assets in release"),
        };
        ctx.notify(&format!(
            "Release {} ships {} assets...",
            self.github_tag_name(release)?,
            assets.len()
        ))
        .await;
        for (i, asset) in assets.iter().enumerate() {
            ctx.notify(&format!(
                "{}-> {}{}\t{:.2}mb\t{}",
                termion::style::Bold,
                i,
                termion::style::Reset,
                f32::from_str(&asset["size"].to_string())? / 1_000_000.0,
                asset["name"],
            ))
            .await;
        }
        let pick = ctx.ask_number(0, assets.len(), "Choose one:").await?;
        match &assets[pick]["browser_download_url"] {
            Value::String(v) => Ok(v.to_string()),
            _ => Err(anyhow!("No browser download url in asset")),
        }
    }

    async fn github_get_latest_release(&self, repo: &str) -> anyhow::Result<Value> {
        let ctx = context("🪐", &self.name).await;
        ctx.notify("Treating package as a GitHub repository").await;
        let url = format!("https://api.github.com/repos/{}/releases/latest", repo);
        let mut response = Request::get(&url)
            .body(())
            .context("Failed to build request body")?
            .send_async()
            .await
            .context(url.clone())?;
        if response.status() != 200 {
            bail!("Status: {}\nURL: {}", &url, response.status())
        }
        Ok(response.json()?)
    }
}

impl std::cmp::PartialEq for Package {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl std::fmt::Display for Package {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} release {:?}", self.name, self.release)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Release {
    Version(String),
    Dated(DateTime<Utc>),
}
