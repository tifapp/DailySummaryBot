use aws_config::meta::region::RegionProviderChain;
use aws_sdk_s3::{Client, primitives::ByteStream};
use lambda_runtime::tracing::error;
use serde_json::Value;
use anyhow::{Result, Context, anyhow};

pub async fn create_json_storage_client() -> Client {
    let region_provider = RegionProviderChain::default_provider().or_else("us-west-2");
    let config = aws_config::from_env().region(region_provider).load().await;
    aws_sdk_s3::Client::new(&config)
}

pub trait JsonStorageClient {
    async fn get_json(&self, key: &str) -> Result<Option<Value>>;
    async fn put_json(&self, key: &str, json_value: &Value) -> Result<()>;
    async fn delete_json(&self, key: &str) -> Result<()>;
}

impl JsonStorageClient for Client {
    async fn delete_json(&self, key: &str) -> Result<()> {
        let resp = self.delete_object()
            .bucket("agilesummary")
            .key(key)
            .send()
            .await;
    
        match resp {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow!("Failed to delete json: {}", e)),
        }
    }

    async fn put_json(&self, key: &str, json_value: &Value) -> Result<()> {
        let json_data = serde_json::to_string(json_value)
            .context("Failed to serialize json data")?;
    
        let resp = self.put_object()
            .bucket("agilesummary")
            .key(key)
            .body(ByteStream::from(json_data.into_bytes()))
            .send()
            .await;
    
        match resp {
            Ok(_) => Ok(()),
            Err(e) => Err(anyhow!("Failed to insert json: {}", e)),
        }
    }

    async fn get_json(&self, key: &str) -> Result<Option<Value>> {
        let object = match self.get_object()
            .bucket("agilesummary")
            .key(key)
            .send()
            .await {
            Ok(data) => data,
            Err(e) => {
                error!("Failed to fetch object from S3: {}", e);
                return Ok(None);
            }
        };
    
        let data = match object.body.collect().await {
            Ok(bytes) => bytes,
            Err(e) => {
                error!("Failed to read object data: {}", e);
                return Ok(None);
            }
        };
    
        match serde_json::from_slice::<Value>(&data.into_bytes()) {
            Ok(json) => Ok(Some(json)),
            Err(e) => {
                error!("Failed to parse JSON data: {}", e);
                Ok(None)
            }
        }
    }
}