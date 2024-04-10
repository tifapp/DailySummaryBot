use aws_sdk_s3::{Client};
use std::collections::HashMap;
use anyhow::{Result, Context};

pub async fn get_s3_json(client: Client, key: &str) -> Result<HashMap<String, String>> {
    let object = client
        .get_object()
        .bucket("agilesummary")
        .key(key)
        .send()
        .await
        .context("should fetch object from S3")?;
    
    let data = object
        .body
        .collect()
        .await
        .context("should read object data")?;
    
    let json = serde_json::from_slice(&data.into_bytes())
        .context("should parse JSON data")?;
    
    Ok(json)
}

// async fn write_s3_file(client: Client, bucket: &str, key: &str) -> Result<Value> {
//     let object = client
//         .get_object()
//         .bucket(bucket)
//         .key(key)
//         .send()
//         .await
//         .context("should fetch object from S3")?;
    
//     let data = object
//         .body
//         .collect()
//         .await
//         .context("should read object data")?;
    
//     let json = serde_json::from_slice(&data.into_bytes())
//         .context("should parse JSON data")?;
    
//     Ok(json)
// }