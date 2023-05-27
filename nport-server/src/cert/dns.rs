use async_trait::async_trait;

pub type Result = anyhow::Result<()>;

#[async_trait]
pub trait Updater {
    async fn update(&self, record: &str, challenge: &str) -> Result;
}
