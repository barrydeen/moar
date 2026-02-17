use heed::types::*;
use heed::{Database, Env, EnvOpenOptions};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobMeta {
    pub sha256: String,
    pub size: u64,
    pub mime_type: String,
    pub uploaded: u64,
    pub uploader: String,
}

#[derive(Clone)]
pub struct BlobStore {
    env: Arc<Env>,
    /// sha256 hex string → BlobMeta as JSON bytes
    blobs_db: Database<Str, Bytes>,
    /// "pubkey:sha256" → unit, for listing by uploader
    uploaders_db: Database<Str, Unit>,
    /// Root directory for blob files
    storage_dir: PathBuf,
}

impl BlobStore {
    pub fn new<P: AsRef<Path>>(storage_path: P) -> crate::error::Result<Self> {
        let storage_dir = storage_path.as_ref().to_path_buf();
        let db_dir = storage_dir.join("db");
        fs::create_dir_all(&db_dir)?;
        fs::create_dir_all(storage_dir.join("blobs"))?;

        let mut env_builder = EnvOpenOptions::new();
        env_builder.max_dbs(5);
        env_builder.map_size(1024 * 1024 * 1024); // 1 GB for metadata
        let env = unsafe { env_builder.open(&db_dir)? };

        let mut wtxn = env.write_txn()?;
        let blobs_db = env.create_database(&mut wtxn, Some("blobs"))?;
        let uploaders_db = env.create_database(&mut wtxn, Some("uploaders"))?;
        wtxn.commit()?;

        Ok(Self {
            env: Arc::new(env),
            blobs_db,
            uploaders_db,
            storage_dir,
        })
    }

    /// Get the filesystem path for a blob, sharded by first 2 hex chars.
    pub fn get_blob_path(&self, sha256: &str) -> PathBuf {
        let prefix = &sha256[..2.min(sha256.len())];
        self.storage_dir.join("blobs").join(prefix).join(sha256)
    }

    pub fn has_blob(&self, sha256: &str) -> crate::error::Result<bool> {
        let rtxn = self.env.read_txn()?;
        Ok(self.blobs_db.get(&rtxn, sha256)?.is_some())
    }

    pub fn get_meta(&self, sha256: &str) -> crate::error::Result<Option<BlobMeta>> {
        let rtxn = self.env.read_txn()?;
        match self.blobs_db.get(&rtxn, sha256)? {
            Some(raw) => Ok(Some(serde_json::from_slice(raw)?)),
            None => Ok(None),
        }
    }

    pub fn save_blob(
        &self,
        sha256: &str,
        data: &[u8],
        mime_type: &str,
        uploader: &str,
    ) -> crate::error::Result<BlobMeta> {
        // Write file to disk
        let blob_path = self.get_blob_path(sha256);
        if let Some(parent) = blob_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&blob_path, data)?;

        let meta = BlobMeta {
            sha256: sha256.to_string(),
            size: data.len() as u64,
            mime_type: mime_type.to_string(),
            uploaded: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            uploader: uploader.to_string(),
        };

        let meta_bytes = serde_json::to_vec(&meta)?;
        let uploader_key = format!("{}:{}", uploader, sha256);

        let mut wtxn = self.env.write_txn()?;
        self.blobs_db.put(&mut wtxn, sha256, &meta_bytes)?;
        self.uploaders_db.put(&mut wtxn, &uploader_key, &())?;
        wtxn.commit()?;

        Ok(meta)
    }

    pub fn list_by_pubkey(&self, pubkey: &str) -> crate::error::Result<Vec<BlobMeta>> {
        let rtxn = self.env.read_txn()?;
        let prefix = format!("{}:", pubkey);
        let mut results = Vec::new();

        let iter = self.uploaders_db.iter(&rtxn)?;
        for result in iter {
            let (key, _) = result?;
            if key.starts_with(&prefix) {
                let sha256 = &key[prefix.len()..];
                if let Some(raw) = self.blobs_db.get(&rtxn, sha256)? {
                    let meta: BlobMeta = serde_json::from_slice(raw)?;
                    results.push(meta);
                }
            }
        }

        results.sort_by(|a, b| b.uploaded.cmp(&a.uploaded));
        Ok(results)
    }

    pub fn list_all(&self) -> crate::error::Result<Vec<BlobMeta>> {
        let rtxn = self.env.read_txn()?;
        let mut results = Vec::new();

        let iter = self.blobs_db.iter(&rtxn)?;
        for result in iter {
            let (_, raw) = result?;
            let meta: BlobMeta = serde_json::from_slice(raw)?;
            results.push(meta);
        }

        results.sort_by(|a, b| b.uploaded.cmp(&a.uploaded));
        Ok(results)
    }

    pub fn delete_blob(&self, sha256: &str) -> crate::error::Result<bool> {
        let rtxn = self.env.read_txn()?;
        let meta = match self.blobs_db.get(&rtxn, sha256)? {
            Some(raw) => {
                let m: BlobMeta = serde_json::from_slice(raw)?;
                m
            }
            None => return Ok(false),
        };
        drop(rtxn);

        // Remove file
        let blob_path = self.get_blob_path(sha256);
        let _ = fs::remove_file(&blob_path);

        // Remove from DB
        let uploader_key = format!("{}:{}", meta.uploader, sha256);
        let mut wtxn = self.env.write_txn()?;
        self.blobs_db.delete(&mut wtxn, sha256)?;
        self.uploaders_db.delete(&mut wtxn, &uploader_key)?;
        wtxn.commit()?;

        Ok(true)
    }
}
