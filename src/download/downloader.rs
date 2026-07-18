use std::{sync::Arc, time::Duration};

use futures::stream::{FuturesUnordered, StreamExt};
use tokio::sync::Semaphore;

use crate::{Error, download::{ProgressState, ResolvedFile, reporter::ProgressReporter}};

const USER_AGENT: &str = "syncraft-core/0.1";

fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .expect("failed to build reqwest client")
}

pub struct Downloader {
    client: reqwest::Client,
    semaphore: Semaphore,
    timeout: Duration,
    retry: usize,
    reporter: Option<Arc<dyn ProgressReporter>>,
}

impl Downloader {
    pub fn new(max_concurrent: usize, timeout: Duration, retry: usize) -> Self {
        Self {
            client: build_client(),
            semaphore: Semaphore::new(max_concurrent),
            timeout,
            retry,
            reporter: None,
        }
    }

    pub fn with_reporter(mut self, reporter: Arc<dyn ProgressReporter>) -> Self {
        self.reporter = Some(reporter);
        self
    }

    pub async fn ensure_all<'a, I>(&self, files: I) -> Result<(), Error>
    where I: IntoIterator<Item = &'a ResolvedFile> {
        let files: Vec<_> = files.into_iter().collect();
        let total = files.len() as u64;

        if let Some(ref reporter) = self.reporter {
            reporter.on_progress(ProgressState {
                stage: "downloading".into(),
                current: 0,
                total,
            });
        }

        let completed = std::sync::atomic::AtomicU64::new(0);
        let mut futures = FuturesUnordered::new();

        for file in files {
            futures.push(async move {
                self.ensure(file).await
            });
        }
        while let Some(result) = futures.next().await {
            result?;
            let done = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            if let Some(ref reporter) = self.reporter {
                reporter.on_progress(ProgressState {
                    stage: "downloading".into(),
                    current: done,
                    total,
                });
            }
        }
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
            match self.client.get(url).timeout(self.timeout).send().await {
                Ok(resp) => match resp.error_for_status() {
                    Ok(res) => return Ok(res),
                    Err(err) => errs.push(err),
                },
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