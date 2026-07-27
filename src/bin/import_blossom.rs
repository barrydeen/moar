use heed::types::{Bytes, Str, Unit};
use heed::{Database, EnvOpenOptions};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BlobInfo {
    url: String,
    sha256: String,
    size: u64,
    #[serde(rename = "type")]
    mime_type: String,
    uploaded: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let storage_dir = PathBuf::from("/opt/moar/blossom");
    let db_dir = storage_dir.join("db");

    let pubkey = "c4f5e7a75a8ce3683d529cff06368439c529e5243c6b125ba68789198856cac7";

    // Fetch blob list from HAVEN
    let url = format!("http://localhost:3355/list/{}", pubkey);
    let resp = reqwest::get(&url).await?;
    let blobs: Vec<BlobInfo> = resp.json().await?;
    println!("Fetched {} blobs from HAVEN", blobs.len());

    // 1. Move files to sharded structure
    let mut moved = 0;
    for blob in &blobs {
        let src = storage_dir.join(&blob.sha256);
        if !src.exists() {
            println!("  Missing: {}", &blob.sha256[..16]);
            continue;
        }
        let prefix = &blob.sha256[..2];
        let dst_dir = storage_dir.join("blobs").join(prefix);
        let dst = dst_dir.join(&blob.sha256);
        if dst.exists() {
            fs::remove_file(&src)?;
            moved += 1;
            continue;
        }
        fs::create_dir_all(&dst_dir)?;
        if let Err(_) = fs::rename(&src, &dst) {
            // Fall back to copy+delete
            fs::copy(&src, &dst)?;
            fs::remove_file(&src)?;
        }
        moved += 1;
    }
    println!("Moved {} files to sharded structure", moved);

    // 2. Open LMDB and write metadata
    fs::create_dir_all(&db_dir)?;
    let env = unsafe {
        EnvOpenOptions::new()
            .max_dbs(5)
            .map_size(1024 * 1024 * 1024)
            .open(&db_dir)?
    };

    let mut wtxn = env.write_txn()?;
    let blobs_db: Database<Str, Bytes> = env.create_database(&mut wtxn, Some("blobs"))?;
    let uploaders_db: Database<Str, Unit> = env.create_database(&mut wtxn, Some("uploaders"))?;

    let mut imported = 0;
    for blob in &blobs {
        let sha256 = &blob.sha256;
        let dst = storage_dir.join("blobs").join(&sha256[..2]).join(sha256);
        if !dst.exists() {
            continue;
        }

        let meta = serde_json::json!({
            "sha256": sha256,
            "size": blob.size,
            "mime_type": blob.mime_type,
            "uploaded": blob.uploaded,
            "uploader": pubkey,
        });
        let meta_bytes = serde_json::to_vec(&meta)?;

        blobs_db.put(&mut wtxn, sha256, &meta_bytes)?;
        let uploader_key = format!("{}:{}", pubkey, sha256);
        uploaders_db.put(&mut wtxn, &uploader_key, &())?;
        imported += 1;
    }

    wtxn.commit()?;
    println!("Imported {} blob records into LMDB", imported);

    Ok(())
}
