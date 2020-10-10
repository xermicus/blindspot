use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use anyhow::Context;
use async_compression::futures::write::{BzDecoder, GzipDecoder, XzDecoder};
use async_std::fs::{copy, create_dir, remove_file, set_permissions, File, OpenOptions};
use async_std::os::unix::fs::OpenOptionsExt;
use async_std::prelude::*;
use async_tar::Archive;
use isahc::config::RedirectPolicy;
use isahc::prelude::*;
use smol::{self, Timer};

use super::{data_path, ui};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Installer {
    pub url: String,
    pub path: PathBuf,
    pub compression: Option<Compression>,
    pub archive: Option<Archived>,
    pub backup: Option<PathBuf>,
}

impl Installer {
    pub async fn install(&mut self, ctx: &ui::Context) -> anyhow::Result<()> {
        let archive = self.guess_archive();
        let compression = self.guess_compression();
        ctx.notify(&format!(
            "Treating file as a {}{:?}{} archive with {}{:?}{} compression",
            termion::style::Bold,
            archive,
            termion::style::Reset,
            termion::style::Bold,
            compression,
            termion::style::Reset,
        ))
        .await;
        let (file, tmp_path) = self.tmp_file().await?;
        ctx.notify(&format!("Fetching {}", &self.url)).await;
        self.download(ctx.clone(), compression.writer(file)).await?;
        if self.path.exists() {
            let filename = self
                .path
                .as_path()
                .file_name()
                .expect("Install path was not a file name");
            let mut target = data_path().await;
            target.push(filename);
            move_exe(&self.path, &target)
                .await
                .context(target.display().to_string())?;
            self.backup = Some(target);
        }
        archive.install(ctx, &tmp_path, &self.path).await
    }

    pub async fn revert(&mut self, ctx: &ui::Context) -> anyhow::Result<(), std::io::Error> {
        if self.backup.is_none() {
            ctx.notify("No backup found for this package, doing nothing")
                .await;
            return Ok(());
        }
        let src = self.backup.as_ref().unwrap();
        ctx.notify(&format!("From {}", src.display())).await;
        ctx.notify(&format!("To {}", self.path.display())).await;
        move_exe(src, &self.path).await?;
        self.backup = None;
        Ok(())
    }

    pub async fn uninstall(&self, ctx: &ui::Context) -> anyhow::Result<(), std::io::Error> {
        if let Some(backup) = &self.backup {
            remove_file(backup).await?;
        }
        ctx.notify(&format!("Deleting file {}", self.path.display()))
            .await;
        remove_file(&self.path).await
    }

    async fn download(
        &self,
        ctx: ui::Context,
        mut body_writer: Pin<Box<dyn async_std::io::Write + Send>>,
    ) -> anyhow::Result<()> {
        let mut response = Request::get(&self.url)
            .metrics(true)
            .redirect_policy(RedirectPolicy::Limit(50))
            .body(())
            .context("Failed to build request body")?
            .send_async()
            .await
            .context("Failed to download file")?;
        let metrics = response.metrics().unwrap().clone();
        let body = response.body_mut();
        let url = self.url.to_string();
        let progresser = smol::spawn(async move {
            loop {
                let progress = metrics.download_progress();
                ctx.progress(progress.0 / 1_000, progress.1 / 1_000, &url)
                    .await;
                if progress.0 == progress.1 {
                    break;
                }
                Timer::after(Duration::from_millis(20)).await;
            }
        });
        async_std::io::copy(body, &mut body_writer).await?;
        body_writer.flush().await?;
        progresser.await;
        Ok(())
    }

    fn guess_archive(&self) -> Archived {
        if let Some(a) = &self.archive {
            return a.clone();
        }
        if self.url.ends_with(".tar") {
            return Archived::Tar;
        }
        if self.url.ends_with(".tar.gz") {
            return Archived::Tar;
        }
        if self.url.ends_with(".tgz") {
            return Archived::Tar;
        }
        if self.url.ends_with(".tar.bz") {
            return Archived::Tar;
        }
        if self.url.ends_with(".tar.bz2") {
            return Archived::Tar;
        }
        if self.url.ends_with(".tbz") {
            return Archived::Tar;
        }
        if self.url.ends_with(".tar.xz") {
            return Archived::Tar;
        }
        if self.url.ends_with(".txz") {
            return Archived::Tar;
        }
        Archived::None
    }

    fn guess_compression(&self) -> Compression {
        if let Some(c) = &self.compression {
            return c.clone();
        }
        if self.url.ends_with(".gz") {
            return Compression::Gzip;
        }
        if self.url.ends_with(".tgz") {
            return Compression::Gzip;
        }
        if self.url.ends_with(".bz") {
            return Compression::Bzip2;
        }
        if self.url.ends_with(".bz2") {
            return Compression::Bzip2;
        }
        if self.url.ends_with(".tbz") {
            return Compression::Bzip2;
        }
        if self.url.ends_with(".xz") {
            return Compression::Xz;
        }
        if self.url.ends_with(".txz") {
            return Compression::Xz;
        }
        Compression::None
    }

    async fn tmp_file(&self) -> anyhow::Result<(File, PathBuf)> {
        let file_name = self
            .path
            .file_name()
            .unwrap_or_else(|| panic!("Invalid filename: {}", self.path.display()));
        let mut tmp_path = std::env::temp_dir();
        tmp_path.push("blindspot");
        if !tmp_path.exists() {
            create_dir(&tmp_path)
                .await
                .unwrap_or_else(|_| panic!("Failed to create tmp dir: {}", &tmp_path.display()))
        }
        tmp_path.push(file_name);
        Ok((
            OpenOptions::new()
                .create(true)
                .write(true)
                .open(&tmp_path)
                .await
                .context("failed to create tmp file")?,
            tmp_path,
        ))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Archived {
    None,
    Tar,
}

impl Archived {
    async fn install(
        &self,
        ctx: &ui::Context,
        src: &PathBuf,
        dest: &PathBuf,
    ) -> anyhow::Result<()> {
        ctx.notify(&format!("Installing into {}", dest.display()))
            .await;
        match &self {
            Archived::None => move_exe(src, dest).await?,
            Archived::Tar => self.install_tar(ctx, src, dest).await?,
        }
        Ok(())
    }

    async fn install_tar(
        &self,
        ctx: &ui::Context,
        src: &PathBuf,
        dest: &PathBuf,
    ) -> anyhow::Result<()> {
        ctx.notify("Choose a file from Tar archive...").await;
        let mut file_index = 0;
        let archive = Archive::new(async_std::fs::File::open(src).await?);
        let mut entries = archive.entries()?;
        while let Some(file) = entries.next().await {
            let f = file?;
            ctx.notify(&format!(
                "{}-> {}{}\t{:.2}mb\t{}",
                termion::style::Bold,
                file_index,
                termion::style::Reset,
                f.header().size()? as f32 / 1_000_000.0,
                f.header().path()?.display()
            ))
            .await;
            file_index += 1;
        }
        let pick = ctx
            .ask_number(0, file_index, "Enter the file number to install:")
            .await?;
        file_index = 0;
        let mut e = Archive::new(async_std::fs::File::open(src).await?).entries()?;
        while let Some(file) = e.next().await {
            let mut f = file?;
            if pick == file_index {
                ctx.notify(&format!("Installing {}", &dest.display())).await;
                let mut target_file = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .mode(0o750)
                    .open(dest)
                    .await?;
                async_std::io::copy(&mut f, &mut target_file).await?;
                break;
            }
            file_index += 1;
        }
        Ok(())
    }

    pub fn variants() -> [&'static str; 2] {
        ["tar", "none"]
    }
}

impl std::str::FromStr for Archived {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tar" => Ok(Archived::Tar),
            "none" => Ok(Archived::None),
            _ => Err(format!("Invalid archive: {}", s)),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Compression {
    None,
    Gzip,
    Bzip2,
    Xz,
}

impl Compression {
    fn writer(&self, file: File) -> Pin<Box<dyn async_std::io::Write + Send>> {
        match self {
            Compression::None => Box::pin(file),
            Compression::Gzip => Box::pin(GzipDecoder::new(file)),
            Compression::Bzip2 => Box::pin(BzDecoder::new(file)),
            Compression::Xz => Box::pin(XzDecoder::new(file)),
        }
    }

    pub fn variants() -> [&'static str; 4] {
        ["gzip", "bzip2", "xz", "none"]
    }
}

impl std::str::FromStr for Compression {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "gzip" => Ok(Compression::Gzip),
            "bzip2" => Ok(Compression::Gzip),
            "xz" => Ok(Compression::Gzip),
            "none" => Ok(Compression::None),
            _ => Err(format!("Invalid compression: {}", s)),
        }
    }
}

async fn move_exe(src: &PathBuf, dest: &PathBuf) -> anyhow::Result<(), std::io::Error> {
    copy(src, dest).await?;
    remove_file(src).await?;
    let meta = dest.metadata()?;
    let mut perm = meta.permissions();
    perm.set_mode(0o750);
    set_permissions(dest, perm).await
}
