use std::time::Duration;

use futures::stream::{FuturesUnordered, StreamExt};
use syncraft_protocol::ResolvedFile;
use tokio::sync::Semaphore;

use crate::Error;

pub struct Downloader {
    client: reqwest::Client,
    semaphore: Semaphore,
    timeout: Duration,
    retry: usize
}

impl Downloader {
    pub fn new(max_concurrent: usize, timeout: Duration, retry: usize) -> Self {
        Self {
            client: reqwest::Client::new(),
            semaphore: Semaphore::new(max_concurrent),
            timeout,
            retry
        }
    }

    pub async fn ensure_all<'a, I>(&self, files: I) -> Result<(), Error>
    where I: IntoIterator<Item = &'a ResolvedFile> {
        let mut futures = FuturesUnordered::new();

        for file in files {
            futures.push(async move {
                self.ensure(file).await
            });
        }
        while let Some(result) = futures.next().await { result?; }
        Ok(())
    }

    async fn ensure(&self, file: &ResolvedFile) -> Result<(), Error> {
        let _permit = self.semaphore.acquire().await
            .map_err(|_err| Error::Other("failed acquire semaphore lock".to_string()))?;
        let mut errs = Vec::new();
        for _ in 0..self.retry {
            if file.verified().await? { 
                return Ok(());
            }
            if let Err(err) = self.download(file).await {
                errs.push(err);
            }
        }
        if file.verified().await? { return Ok(()); }
        if !errs.is_empty() {
            Err(errs.remove(0))?
        } else {
            Err(Error::Other("0 retry count".to_string()))
        }
    }

    async fn download(&self, file: &ResolvedFile) -> Result<(), Error> {
        let url = file.url.as_ref()
            .ok_or_else(|| Error::Other("missing file url".to_string()))?;
        if let Some(parent) = file.path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let bytes = self.fetch(url).await?.bytes().await?;
        tokio::fs::write(&file.path, &bytes).await?;
        Ok(())
    }

    async fn fetch(&self, urls: &Vec<String>) -> Result<reqwest::Response, Error> {
        let mut errs = Vec::new();
        for url in urls {
            match self.client.get(url).timeout(self.timeout).send().await?.error_for_status() {
                Ok(res) => return Ok(res),
                Err(err) => errs.push(err)
            }
        }
        if !errs.is_empty() {
            Err(errs.remove(0))?
        } else {
            Err(Error::Other("empty url list".to_string()))
        }
    }
}