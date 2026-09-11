use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub engine: String,
    pub tag: String,
    pub image: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub entries: Vec<CatalogEntry>,
}

pub fn bundled_catalog() -> Catalog {
    let raw = include_str!("../../src/catalog.json");
    serde_json::from_str(raw).expect("bundled catalog.json must parse")
}

#[tauri::command]
pub fn get_catalog() -> Result<Catalog, String> {
    Ok(bundled_catalog())
}

pub fn base_port(engine: &str) -> u16 {
    match engine {
        "postgres" => 5432,
        "redis" | "valkey" => 6379,
        _ => 3306,
    }
}

#[tauri::command]
pub fn suggest_port(engine: String, taken: Vec<u16>) -> Result<u16, String> {
    let mut p = base_port(&engine);
    while taken.contains(&p) {
        p += 1;
        if p > 65530 {
            return Err("no free ports".to_string());
        }
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_has_latest_tags() {
        let c = bundled_catalog();
        assert!(c.entries.iter().any(|e| e.image == "mysql:8.4"));
        assert!(c.entries.iter().any(|e| e.image == "postgres:17"));
        assert!(c.entries.iter().any(|e| e.image == "mariadb:11.4"));
    }
}
