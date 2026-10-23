//! Curated GGUF catalog shared by the downloader command and its tests.
//!
//! Kept outside `commands` because the command module is compiled out of the
//! unit-test harness; this allowlist is security-relevant and should stay under
//! test.

use std::path::Path;

use serde::Serialize;

/// One curated model. Repository, revision, and filename are constants; only
/// the id crosses the IPC boundary.
pub struct CatalogEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub filename: &'static str,
    pub repo: &'static str,
    pub size_bytes: u64,
    pub description: &'static str,
}

pub const CATALOG: [CatalogEntry; 3] = [
    CatalogEntry {
        id: "qwen2.5-0.5b-instruct",
        name: "Qwen2.5 0.5B Instruct",
        filename: "qwen2.5-0.5b-instruct-q4_k_m.gguf",
        repo: "Qwen/Qwen2.5-0.5B-Instruct-GGUF",
        size_bytes: 491_400_032,
        description: "Smallest and fastest; good for older CPUs. Weaker synthesis.",
    },
    CatalogEntry {
        id: "qwen2.5-1.5b-instruct",
        name: "Qwen2.5 1.5B Instruct",
        filename: "qwen2.5-1.5b-instruct-q4_k_m.gguf",
        repo: "Qwen/Qwen2.5-1.5B-Instruct-GGUF",
        size_bytes: 1_117_320_736,
        description: "Recommended balance of answer quality and CPU speed.",
    },
    CatalogEntry {
        id: "qwen2.5-3b-instruct",
        name: "Qwen2.5 3B Instruct",
        filename: "qwen2.5-3b-instruct-q4_k_m.gguf",
        repo: "Qwen/Qwen2.5-3B-Instruct-GGUF",
        size_bytes: 2_104_932_768,
        description: "Best answers of the curated set; needs about 4 GB of free RAM.",
    },
];

/// Catalog row plus whether the exact filename already sits in the models
/// folder. The UI shows one card per row.
#[derive(Debug, Clone, Serialize)]
pub struct CatalogModel {
    pub id: String,
    pub name: String,
    pub filename: String,
    pub size_bytes: u64,
    pub description: String,
    pub installed: bool,
}

pub fn catalog_entry(id: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|entry| entry.id == id)
}

pub fn download_url(entry: &CatalogEntry) -> String {
    format!(
        "https://huggingface.co/{}/resolve/main/{}",
        entry.repo, entry.filename
    )
}

/// Redirects are followed only to Hugging Face's own hosts. The initial URL is
/// always huggingface.co; the CDN hosts are `*.hf.co` and
/// `*.huggingface.co`. A redirect anywhere else is refused.
pub fn is_allowed_download_host(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host == "huggingface.co"
        || host.ends_with(".huggingface.co")
        || host == "hf.co"
        || host.ends_with(".hf.co")
}

fn is_installed(dir: &Path, entry: &CatalogEntry) -> bool {
    dir.join(entry.filename).is_file()
}

pub fn catalog_models(dir: &Path) -> Vec<CatalogModel> {
    CATALOG
        .iter()
        .map(|entry| CatalogModel {
            id: entry.id.to_owned(),
            name: entry.name.to_owned(),
            filename: entry.filename.to_owned(),
            size_bytes: entry.size_bytes,
            description: entry.description.to_owned(),
            installed: is_installed(dir, entry),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    #[test]
    fn catalog_ids_are_unique_and_urls_are_hugging_face_https() {
        let mut ids: Vec<&str> = CATALOG.iter().map(|entry| entry.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), CATALOG.len());

        for entry in &CATALOG {
            let url = download_url(entry);
            assert!(url.starts_with("https://huggingface.co/Qwen/"));
            assert!(url.ends_with(entry.filename));
            assert!(entry.filename.ends_with(".gguf"));
            assert!(!entry.filename.contains('/'));
            assert!(!entry.filename.contains('\\'));
            assert!(entry.size_bytes > 0);
        }
    }

    #[test]
    fn unknown_catalog_ids_are_rejected() {
        assert!(catalog_entry("qwen2.5-1.5b-instruct").is_some());
        assert!(catalog_entry("../../etc/passwd").is_none());
        assert!(catalog_entry("qwen2.5-7b-instruct").is_none());
    }

    #[test]
    fn redirects_stay_on_hugging_face_hosts() {
        assert!(is_allowed_download_host("huggingface.co"));
        assert!(is_allowed_download_host("cdn-lfs.huggingface.co"));
        assert!(is_allowed_download_host("cas-bridge.xethub.hf.co"));
        assert!(is_allowed_download_host("HF.CO"));

        assert!(!is_allowed_download_host("huggingface.co.evil.example"));
        assert!(!is_allowed_download_host("evil-hf.co"));
        assert!(!is_allowed_download_host("example.com"));
        assert!(!is_allowed_download_host(""));
    }

    #[test]
    fn catalog_marks_matching_filenames_as_installed() {
        let dir = std::env::temp_dir().join(format!("lra-catalog-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let entry = catalog_entry("qwen2.5-0.5b-instruct").unwrap();
        let file = dir.join(entry.filename);
        std::fs::write(&file, b"stub").unwrap();

        let rows = catalog_models(&dir);
        assert!(rows
            .iter()
            .find(|row| row.id == "qwen2.5-0.5b-instruct")
            .map(|row| row.installed)
            .unwrap_or(false));
        assert!(!rows
            .iter()
            .find(|row| row.id == "qwen2.5-3b-instruct")
            .map(|row| row.installed)
            .unwrap_or(true));

        let _ = std::fs::remove_file(&file);
        let _ = std::fs::remove_dir(&dir);
    }
}
