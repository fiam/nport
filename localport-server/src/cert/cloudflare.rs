use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::dns;

#[derive(Serialize)]
struct CreateOrUpdateRequest<'a> {
    #[serde(rename = "type")]
    _type: &'a str,
    #[serde(rename = "name")]
    name: &'a str,
    #[serde(rename = "content")]
    content: &'a str,
    #[serde(rename = "ttl")]
    ttl: u32,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct ResponseError {
    #[serde(rename = "code")]
    pub code: i32,
    #[serde(rename = "message")]
    pub message: String,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct QueryResult {
    #[serde(rename = "id")]
    pub id: String,
    #[serde(rename = "zone_id")]
    pub zone_id: String,
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "type")]
    pub _type: String,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct QueryResponse {
    #[serde(rename = "success")]
    pub success: bool,
    #[serde(rename = "errors")]
    pub errors: Vec<ResponseError>,
    #[serde(rename = "result")]
    pub result: Vec<QueryResult>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct CreateOrUpdateResponse {
    #[serde(rename = "success")]
    pub success: bool,
    #[serde(rename = "errors")]
    pub errors: Vec<ResponseError>,
}

pub struct Updater {
    token: String,
    zone_id: String,
}

impl Updater {
    pub fn new(token: &str, zone_id: &str) -> Self {
        Self {
            token: token.to_string(),
            zone_id: zone_id.to_string(),
        }
    }

    fn get_create_request<'a>(
        &self,
        record: &'a str,
        challenge: &'a str,
    ) -> CreateOrUpdateRequest<'a> {
        CreateOrUpdateRequest {
            _type: "TXT",
            name: record,
            content: challenge,
            ttl: 60,
        }
    }

    async fn get_record_id(
        &self,
        client: &reqwest::Client,
        record: &str,
        authorization: &str,
    ) -> Result<String, anyhow::Error> {
        let url = format!(
            "https://api.cloudflare.com/client/v4/zones/{}/dns_records?match=all&name={}&type=TXT",
            self.zone_id, record
        );
        let resp = client
            .get(url)
            .header("Authorization", authorization)
            .send()
            .await?;

        let body: QueryResponse = resp.json().await?;
        if body.success {
            for result in body.result {
                if result.name == record && result._type == "TXT" && !result.id.is_empty() {
                    return Ok(result.id);
                }
            }
        }
        Err(anyhow::anyhow!("record not found"))
    }

    async fn update_record(
        &self,
        client: &reqwest::Client,
        record: &str,
        record_id: &str,
        challenge: &str,
        authorization: &str,
    ) -> dns::Result {
        debug!(record_id, "updating TXT record");
        let url = format!(
            "https://api.cloudflare.com/client/v4/zones/{}/dns_records/{}",
            self.zone_id, record_id
        );

        let request = self.get_create_request(record, challenge);

        let resp = client
            .put(url)
            .header("Authorization", authorization)
            .json(&request)
            .send()
            .await?;

        let body: CreateOrUpdateResponse = resp.json().await?;

        if !body.success {
            return Err(anyhow::anyhow!("updating TXT record: {:?}", body.errors));
        }
        Ok(())
    }

    async fn create_record(
        &self,
        client: &reqwest::Client,
        record: &str,
        challenge: &str,
        authorization: &str,
    ) -> dns::Result {
        debug!(record, "create record");
        let url = format!(
            "https://api.cloudflare.com/client/v4/zones/{}/dns_records",
            self.zone_id
        );
        let request = self.get_create_request(record, challenge);
        let resp = client
            .post(url)
            .header("Authorization", authorization)
            .json(&request)
            .send()
            .await?;

        let body: CreateOrUpdateResponse = resp.json().await?;

        if !body.success {
            return Err(anyhow::anyhow!("creating TXT record: {:?}", body.errors));
        }
        Ok(())
    }
}

#[async_trait]
impl dns::Updater for Updater {
    async fn update(&self, record: &str, challenge: &str) -> dns::Result {
        let authorization = format!("Bearer {}", self.token);
        let client = reqwest::Client::new();

        if let Ok(id) = self.get_record_id(&client, record, &authorization).await {
            if let Ok(result) = self
                .update_record(&client, record, &id, challenge, &authorization)
                .await
            {
                return Ok(result);
            }
        }
        self.create_record(&client, record, challenge, &authorization)
            .await
    }
}
