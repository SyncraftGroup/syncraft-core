use std::time::Duration;

use futures::stream::{FuturesUnordered, StreamExt};
use tokio::{io::AsyncWriteExt, sync::Semaphore};

use crate::{
    Error,
    download::{ResolvedFile, send_retrying},
    progress::Progress,
};

const USER_AGENT: &str = "syncraft-core/0.1";

fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .redirect(reqwest::redirect::Policy::limited(10))
        // Parallel downloads work better with separate HTTP/1.1 connections:
        // multiplexed HTTP/2 streams are throttled by the CDN's flow control.
        .http1_only()
        .connect_timeout(Duration::from_secs(15))
        // A stalled mirror must not occupy a concurrency slot for the whole
        // overall timeout; abort when no bytes arrive for a while.
        .read_timeout(Duration::from_secs(30))
        .build()
        .expect("failed to build reqwest client")
}

pub struct Downloader {
    client: reqwest::Client,
    semaphore: Semaphore,
    /// Total time allowed for one file. Large assets over slow links need a
    /// generous value; the connect timeout above catches unreachable hosts.
    timeout: Duration,
    retry: usize,
}

impl Downloader {
    pub fn new(max_concurrent: usize, timeout: Duration, retry: usize) -> Self {
        Self {
            client: build_client(),
            semaphore: Semaphore::new(max_concurrent.max(1)),
            timeout,
            retry,
        }
    }

    pub async fn ensure_all<'a, I>(
        &self,
        files: I,
        progress: Option<&dyn Progress>,
    ) -> Result<(), Error>
    where
        I: IntoIterator<Item = &'a ResolvedFile>,
    {
        let files: Vec<_> = files.into_iter().collect();

        if let Some(p) = progress {
            p.add_total(files.len() as u64);
        }

        let mut futures = FuturesUnordered::new();

        for file in files {
            let name = file
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            let size = file.size.unwrap_or(0) as u64;
            if let Some(p) = progress {
                p.file_start(name, size);
            }

            futures.push(async move { self.ensure(file, name, progress).await });
        }

        while let Some(result) = futures.next().await {
            result?;
            if let Some(p) = progress {
                p.overall_inc();
            }
        }

        Ok(())
    }

    async fn ensure(
        &self,
        file: &ResolvedFile,
        name: &str,
        progress: Option<&dyn Progress>,
    ) -> Result<(), Error> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|_err| Error::Other("failed acquire semaphore lock".to_string()))?;

        let mut errs = Vec::new();
        for _ in 0..self.retry {
            if file.verified().await? {
                if let Some(p) = progress {
                    p.file_done(name);
                }
                return Ok(());
            }
            if let Err(err) = self.download(file, name, progress).await {
                errs.push(err);
            }
        }
        if file.verified().await? {
            if let Some(p) = progress {
                p.file_done(name);
            }
            return Ok(());
        }
        if !errs.is_empty() {
            Err(errs.remove(0))?
        } else {
            Err(Error::Other("0 retry count".to_string()))
        }
    }

    async fn download(
        &self,
        file: &ResolvedFile,
        name: &str,
        progress: Option<&dyn Progress>,
    ) -> Result<(), Error> {
        let urls = file
            .url
            .as_ref()
            .ok_or_else(|| Error::Other("missing file url".to_string()))?;
        if let Some(parent) = file.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let response = self.fetch(urls).await?;
        let total_size = response.content_length().unwrap_or(0);
        if let Some(p) = progress {
            p.file_total(name, total_size);
        }
        let mut stream = response.bytes_stream();
        let mut dest = tokio::fs::File::create(&file.path).await?;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            dest.write_all(&chunk).await?;
            if let Some(p) = progress {
                p.file_inc(name, chunk.len() as u64);
            }
        }
        dest.sync_all().await?;
        Ok(())
    }

    async fn fetch(&self, urls: &Vec<String>) -> Result<reqwest::Response, Error> {
        let mut errs = Vec::new();
        for url in urls {
            match send_retrying(|| self.client.get(url).timeout(self.timeout)).await {
                Ok(resp) => return Ok(resp),
                Err(err) => errs.push(err),
            }
        }
        if !errs.is_empty() {
            Err(errs.remove(0))?
        } else {
            Err(Error::Other("empty url list".to_string()))
        }
    }
}
