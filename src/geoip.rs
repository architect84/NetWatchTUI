use maxminddb::Reader;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

const GEODB_URL: &str = "https://cdn.jsdelivr.net/npm/@ip-location-db/geo-whois-asn-country-mmdb/geo-whois-asn-country.mmdb";

/// Geolocation result for a single IP.
#[derive(Debug, Clone, Default)]
pub struct GeoInfo {
    pub country_code: Option<String>,
    pub country_name: Option<String>,
    pub city: Option<String>,
}

impl std::fmt::Display for GeoInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let country = self
            .country_code
            .as_deref()
            .unwrap_or("-");
        write!(f, "{}", country)
    }
}

/// GeoIP lookup engine backed by a MaxMind-format MMDB file.
pub struct GeoIp {
    reader: Reader<Vec<u8>>,
}

/// Serde structs for deserializing MMDB records.
/// Supports both MaxMind GeoLite2-City and IPLocate country-only formats.
#[derive(serde::Deserialize, Debug)]
struct MmdbRecord<'a> {
    #[serde(borrow)]
    country: Option<MmdbCountry<'a>>,
    #[serde(borrow)]
    city: Option<MmdbCity<'a>>,
    country_code: Option<&'a str>,
    country_name: Option<&'a str>,
}

#[derive(serde::Deserialize, Debug)]
struct MmdbCountry<'a> {
    iso_code: Option<&'a str>,
    #[serde(borrow)]
    names: Option<MmdbNames<'a>>,
}

#[derive(serde::Deserialize, Debug)]
struct MmdbCity<'a> {
    #[serde(borrow)]
    names: Option<MmdbNames<'a>>,
}

#[derive(serde::Deserialize, Debug)]
struct MmdbNames<'a> {
    en: Option<&'a str>,
}

impl GeoIp {
    /// Open a MMDB file for lookups.
    pub fn open(path: &Path) -> Result<Self, String> {
        let reader = Reader::open_readfile(path)
            .map_err(|e| format!("Failed to open GeoIP database '{}': {}", path.display(), e))?;
        Ok(Self { reader })
    }

    /// Look up geolocation info for an IP address.
    pub fn lookup(&self, addr: &IpAddr) -> GeoInfo {
        let result = self.reader.lookup(*addr);
        let lookup = match result {
            Ok(r) => r,
            Err(_) => return GeoInfo::default(),
        };

        // Try to decode as our flexible record type
        let record: Option<MmdbRecord> = lookup.decode().ok().flatten();

        match record {
            Some(rec) => {
                // IPLocate format: top-level country_code/country_name
                // MaxMind format: nested country.iso_code, country.names.en, city.names.en
                let country_code = rec
                    .country_code
                    .map(String::from)
                    .or_else(|| {
                        rec.country
                            .as_ref()
                            .and_then(|c| c.iso_code.map(String::from))
                    });

                let country_name = rec
                    .country_name
                    .map(String::from)
                    .or_else(|| {
                        rec.country
                            .as_ref()
                            .and_then(|c| c.names.as_ref())
                            .and_then(|n| n.en.map(String::from))
                    });

                let city = rec
                    .city
                    .as_ref()
                    .and_then(|c| c.names.as_ref())
                    .and_then(|n| n.en.map(String::from));

                GeoInfo {
                    country_code,
                    country_name,
                    city,
                }
            }
            None => GeoInfo::default(),
        }
    }
}

/// Return the default database path: ~/.local/share/netwatch/ip-to-country.mmdb
pub fn default_db_path() -> PathBuf {
    let data_dir = dirs_fallback();
    data_dir.join("ip-to-country.mmdb")
}

fn dirs_fallback() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("netwatch")
    } else {
        PathBuf::from("/tmp/netwatch")
    }
}

/// Download the free IPLocate IP-to-Country MMDB database.
pub fn download_db(dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory '{}': {}", parent.display(), e))?;
    }

    eprintln!("Downloading GeoIP database from {}...", GEODB_URL);

    let response = reqwest::blocking::get(GEODB_URL)
        .map_err(|e| format!("Download failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Download failed with HTTP status: {}",
            response.status()
        ));
    }

    let bytes = response
        .bytes()
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    std::fs::write(dest, &bytes)
        .map_err(|e| format!("Failed to write database to '{}': {}", dest.display(), e))?;

    eprintln!(
        "Database saved to {} ({:.1} MB)",
        dest.display(),
        bytes.len() as f64 / 1_048_576.0
    );

    Ok(())
}
