use local_ip_address::local_ip;
use migrations::HH_MIGRATIONS;
use once_cell::sync::Lazy;
use poem::{
    error::{ExpectationFailed, InternalServerError},
    get, handler,
    listener::{Listener, RustlsCertificate, RustlsConfig, TcpListener},
    post,
    web::{Json, Path, Query},
    IntoResponse, Result, Route, Server,
};
use rcgen::{generate_simple_self_signed, CertifiedKey};
use serde::{Deserialize, Serialize};
use serde_json::{self, json};
use sqlx::sqlite::SqlitePool;
use sqlx::Row;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{collections::HashMap, net::IpAddr, path::PathBuf, io::BufReader, fs::File};
use tauri::State;
use tauri_plugin_fs::FsExt;
use tauri_plugin_sql::{Builder, DbInstances, DbPool, Migration, MigrationKind};

use futures::future::try_join_all;

#[path = "util/db.rs"]
mod db;

mod migrations;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[handler]
fn hello(Path(name): Path<String>) -> String {
    println!("Hello: {name}");
    format!("Hello, {}!", name)
}

#[handler]
fn index_route() -> String {
    println!("Index route accessed");
    "Hello hikma health local server.".to_string()
}

/// Represents a single record in a table with its raw data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawRecord {
    pub id: String,
    pub created_at: i64,
    pub updated_at: i64,
    // Other common fields could be added here - Should we??
    // Flatten the fields to support dynamic fields
    // TODO: Actually, does this do what I think it does. @ally review
    #[serde(flatten)]
    pub data: HashMap<String, serde_json::Value>,
}

/// Represents changes to a single table, categorized by operation type
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncTableChangeSet {
    pub created: Vec<RawRecord>,
    pub updated: Vec<RawRecord>,
    pub deleted: Vec<String>, // List of IDs to delete
}

/// Represents changes to the entire database, organized by table name
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncDatabaseChangeSet(HashMap<String, SyncTableChangeSet>);

impl SyncDatabaseChangeSet {
    /// Create a new empty change set
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Add a table's changes to the database change set
    pub fn add_table_changes(&mut self, table_name: &str, changes: SyncTableChangeSet) {
        if !changes.is_empty() {
            self.0.insert(table_name.to_string(), changes);
        }
    }

    /// Get changes for a specific table
    pub fn get_table_changes(&self, table_name: &str) -> Option<&SyncTableChangeSet> {
        self.0.get(table_name)
    }

    /// Get all table names that have changes
    pub fn table_names(&self) -> Vec<&String> {
        self.0.keys().collect()
    }

    /// Check if there are any changes in the database
    pub fn is_empty(&self) -> bool {
        self.0.is_empty() || self.0.values().all(|changes| changes.is_empty())
    }
}

impl SyncTableChangeSet {
    /// Create a new empty table change set
    pub fn new() -> Self {
        Self {
            created: Vec::new(),
            updated: Vec::new(),
            deleted: Vec::new(),
        }
    }

    /// Check if there are any changes in this table
    pub fn is_empty(&self) -> bool {
        self.created.is_empty() && self.updated.is_empty() && self.deleted.is_empty()
    }

    /// Get the total number of changes in this table
    pub fn total_changes(&self) -> usize {
        self.created.len() + self.updated.len() + self.deleted.len()
    }

    /// Filter records by timestamp
    pub fn filter_by_timestamp(&self, timestamp: i64) -> Self {
        Self {
            created: self
                .created
                .iter()
                .filter(|record| record.created_at >= timestamp)
                .cloned()
                .collect(),
            updated: self
                .updated
                .iter()
                .filter(|record| record.created_at < timestamp && record.updated_at >= timestamp)
                .cloned()
                .collect(),
            deleted: self.deleted.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LocalDBEntry {
    id: String,
    created_at: i64,
    updated_at: i64,
    local_server_created_at: i64,
    local_server_last_modified: i64,
    data: HashMap<String, serde_json::Value>, // This is stored as a JSON object
}

// Tables sent from the syncing clients include:
// is it possible to make collections on the fly if they dont exist?
// TODO: @ally consider how this could work.
// const tables = [
//     "patients",
//     "events",
//     "visits",
//     "clinics",
//     "users",
//     "event_forms",
//     "registration_forms",
//     "patient_additional_attributes",
//     "appointments",
//     "prescriptions",
//   ]

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GetSyncResponse {
    changes: SyncDatabaseChangeSet,
    timestamp: i64,
}

#[handler]
async fn get_sync(Query(params): Query<HashMap<String, String>>) -> Result<impl IntoResponse> {
    println!("GET sync request: {:#?}", params);
    let last_pulled_at = params
        .get("lastPulledAt")
        .map(|timestamp| {
            println!("GET sync request with lastPulledAt: {}", timestamp);
            timestamp.parse::<i64>().unwrap_or(0)
        })
        .unwrap_or_else(|| {
            println!("GET sync request with no lastPulledAt parameter");
            0
        });

    // Get database connection
    let db_url = db::get_database();

    let db_pool = match SqlitePool::connect(&db_url).await {
        Ok(pool) => pool,
        Err(err) => return Err(InternalServerError(err)),
    };

    // Define the tables to query
    let tables = vec![
        "patients", "events", "visits", "clinics", "users", 
        "event_forms", "registration_forms", "patient_additional_attributes", 
        "appointments", "prescriptions"
    ];

    // Create a future for each table to query in parallel
    let table_futures = tables
        .into_iter()
        .map(|table| {
            let db_pool = db_pool.clone();
            let table_name = table.to_string();
            
            async move {
                // Create a new change set for this table
                let mut table_changes = SyncTableChangeSet::new();
                
                // Query for new records (created after last sync)
                let new_records_sql = format!(
                    "SELECT id, data, local_server_created_at, local_server_last_modified FROM {} 
                     WHERE local_server_created_at > ? 
                     AND local_server_deleted_at IS NULL", 
                    table_name
                );
                
                let new_records_result = sqlx::query(&new_records_sql)
                    .bind(last_pulled_at)
                    .fetch_all(&db_pool)
                    .await;
                
                if let Ok(rows) = new_records_result {
                    let created_records = rows.into_iter()
                        .filter_map(|row| {
                            let id: String = row.get("id");
                            let data_json: String = row.get("data");
                            let created_at: i64 = row.get("local_server_created_at");
                            let updated_at: i64 = row.get("local_server_last_modified");
                            
                            // Parse the JSON data
                            match serde_json::from_str::<HashMap<String, serde_json::Value>>(&data_json) {
                                Ok(mut data) => {
                                    // Add the id to the data map
                                    data.insert("id".to_string(), serde_json::Value::String(id.clone()));
                                    
                                    Some(RawRecord {
                                        id,
                                        created_at,
                                        updated_at,
                                        data,
                                    })
                                },
                                Err(e) => {
                                    eprintln!("Failed to parse JSON for record {}: {}", id, e);
                                    None
                                }
                            }
                        })
                        .collect();
                    
                    table_changes.created = created_records;
                }
                
                // Query for updated records (modified after last sync but created before)
                let updated_records_sql = format!(
                    "SELECT id, data, local_server_created_at, local_server_last_modified FROM {} 
                     WHERE local_server_last_modified > ? 
                     AND local_server_created_at <= ? 
                     AND local_server_deleted_at IS NULL", 
                    table_name
                );
                
                let updated_records_result = sqlx::query(&updated_records_sql)
                    .bind(last_pulled_at)
                    .bind(last_pulled_at)
                    .fetch_all(&db_pool)
                    .await;
                
                if let Ok(rows) = updated_records_result {
                    let updated_records = rows.into_iter()
                        .filter_map(|row| {
                            let id: String = row.get("id");
                            let data_json: String = row.get("data");
                            let created_at: i64 = row.get("local_server_created_at");
                            let updated_at: i64 = row.get("local_server_last_modified");
                            
                            // Parse the JSON data
                            match serde_json::from_str::<HashMap<String, serde_json::Value>>(&data_json) {
                                Ok(mut data) => {
                                    // Add the id to the data map
                                    data.insert("id".to_string(), serde_json::Value::String(id.clone()));
                                    
                                    Some(RawRecord {
                                        id,
                                        created_at,
                                        updated_at,
                                        data,
                                    })
                                },
                                Err(e) => {
                                    eprintln!("Failed to parse JSON for record {}: {}", id, e);
                                    None
                                }
                            }
                        })
                        .collect();
                    
                    table_changes.updated = updated_records;
                }
                
                // Query for deleted records
                let deleted_records_sql = format!(
                    "SELECT id FROM {} WHERE local_server_deleted_at > ?", 
                    table_name
                );
                
                let deleted_records_result = sqlx::query(&deleted_records_sql)
                    .bind(last_pulled_at)
                    .fetch_all(&db_pool)
                    .await;
                
                if let Ok(rows) = deleted_records_result {
                    let deleted_ids = rows.into_iter()
                        .map(|row| row.get::<String, _>("id"))
                        .collect();
                    
                    table_changes.deleted = deleted_ids;
                }

                // Return the table name and changes as a Result
                // This makes it compatible with try_join_all
                Ok::<_, poem::Error>((table_name, table_changes))
                
                // // Return the table name and changes if there are any
                // if !table_changes.is_empty() {
                //     Some((table_name, table_changes))
                // } else {
                //     None
                // }
            }
        })
        .collect::<Vec<_>>();

     // Execute all futures in parallel and collect the results
     let table_results = try_join_all(table_futures).await?;
    
     // Create the database change set from the results using functional approach
     let mut changes = SyncDatabaseChangeSet::new();
 
     // Add each table's changes to the database change set if not empty
     table_results.into_iter()
         .filter(|(_, table_changes)| !table_changes.is_empty())
         .for_each(|(table_name, table_changes)| {
             changes.add_table_changes(&table_name, table_changes);
         });
 
     println!(
         "GET sync response: {}",
         serde_json::to_string_pretty(&changes)
             .unwrap_or_else(|_| "Failed to serialize changes".to_string())
     );
    
    Ok(Json(GetSyncResponse { changes, timestamp: timestamp() }))
}

#[handler]
async fn post_sync(
    Json(body): Json<SyncDatabaseChangeSet>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse> {
    println!(
        "POST sync request body: {}",
        serde_json::to_string_pretty(&body)
            .unwrap_or_else(|_| "Failed to serialize body".to_string())
    );
    params
        .get("lastPulledAt")
        .map(|timestamp| println!("POST sync request with lastPulledAt: {}", timestamp))
        .unwrap_or_else(|| println!("POST sync request with no lastPulledAt parameter"));

    println!(
        "Last Pulled At: {}",
        params.get("lastPulledAt").unwrap_or(&"0".to_string())
    );

    let db_url = db::get_database();

    let ignored_tables = vec!["users".to_string(), "registration_forms".to_string(), "event_forms".to_string()];

    let db_pool = match SqlitePool::connect(&db_url).await {
        Ok(pool) => pool,
        Err(err) => return Err(InternalServerError(err)),
    };
    // Count the changes in each table
    let table_futures = body
        .0
        .iter()
        .filter(|(table, _)| !ignored_tables.contains(table))
        .map(|(table, changeset)| {
            let table = table.clone();
            let db_pool = db_pool.clone();
            
            async move {
                println!("Table: {}", table);
                println!("Created: {:#?}", changeset.created.len());
                println!("Updated: {:#?}", changeset.updated.len());

                // If there are records in the created field, join them with the updated records
                let updated_records = changeset
                    .created
                    .iter()
                    .chain(changeset.updated.iter())
                    .map(|rec| record_to_local_db_entry(rec))
                    .collect::<Vec<LocalDBEntry>>();

                // Process updates/inserts using upserts for the table
                for entry in &updated_records {
                    // Serialize the data field to a JSON string
                    let data_json = match serde_json::to_string(&entry.data) {
                        Ok(json) => json,
                        Err(e) => {
                            eprintln!(
                                "Failed to serialize data for ID {} in table {}: {}",
                                entry.id, table, e
                            );
                            return Err(InternalServerError(e));
                        }
                    };

                    // Construct the dynamic SQL query for upsert
                    let sql = format!(
                        r#"
                        INSERT INTO "{}" (id, data, local_server_created_at, local_server_last_modified)
                        VALUES (?, ?, ?, ?)
                        ON CONFLICT(id) DO UPDATE SET
                            data = excluded.data,
                            local_server_last_modified = excluded.local_server_last_modified
                        WHERE excluded.local_server_last_modified > "{}" .local_server_last_modified;
                        "#,
                        table, table
                    );

                    // Block on the async execution for simplicity in the map closure
                    let query_result = 
                        sqlx::query(&sql)
                            .bind(&entry.id)
                            .bind(&data_json)
                            .bind(entry.local_server_created_at)
                            .bind(entry.local_server_last_modified)
                            .execute(&db_pool) // Use the pool cloned earlier in the handler
                            .await;

                    match query_result {
                        Ok(_) => {} // Successfully upserted
                        Err(e) => {
                            // Log the error and return an error to stop processing this table
                            eprintln!(
                                "Failed to upsert record with ID {} in table {}: {}",
                                entry.id, table, e
                            );
                            return Err(InternalServerError(e));
                        }
                    }
                }

                // Process deletes (soft delete)
                let current_time = timestamp(); // Get timestamp once for all deletions in this table
                for deleted_id in &changeset.deleted {
                    // Use a single upsert statement to either create a new record marked as deleted
                    // or update an existing record to mark it as deleted
                    let sql_upsert_delete = format!(
                        r#"
                        INSERT INTO "{}" (id, data, local_server_created_at, local_server_last_modified, local_server_deleted_at)
                        VALUES (?, '{{}}', ?, ?, ?)
                        ON CONFLICT(id) DO UPDATE SET
                            local_server_last_modified = excluded.local_server_last_modified,
                            local_server_deleted_at = COALESCE(
                                "{0}".local_server_deleted_at, 
                                excluded.local_server_deleted_at
                            );
                        "#,
                        table
                    );

                    let delete_result = 
                        sqlx::query(&sql_upsert_delete)
                            .bind(deleted_id)
                            .bind(current_time)
                            .bind(current_time)
                            .bind(current_time)
                            .execute(&db_pool)
                            .await;

                     match delete_result {
                         Ok(_) => {
                             // Successfully marked as deleted (either new or existing record)
                         }
                         Err(e) => {
                             // Log the error and return an error to stop processing this table
                             eprintln!(
                                 "Failed to soft-delete record with ID {} in table {}: {}",
                                 deleted_id, table, e
                             );
                             return Err(InternalServerError(e));
                         }
                     }
                }

                // Process the updated and deleted records for this table
                let mut update_count = 0;
                let mut delete_count = 0;

                if !changeset.updated.is_empty() {
                    update_count = changeset.updated.len();
                }

                if !changeset.deleted.is_empty() {
                    delete_count = changeset.deleted.len();
                }

                Ok((table.clone(), (update_count, delete_count)))
            }
        })
        .collect::<Vec<_>>();
    // .collect::<Result<HashMap<String, (usize, usize)>, poem::Error>>()?;

    // Execute all futures in parallel and collect the results
    let table_changes = try_join_all(table_futures).await?
        .into_iter()
        .collect::<HashMap<String, (usize, usize)>>();


    // Calculate totals
    let update_count: usize = table_changes.values().map(|(updates, _)| updates).sum();
    let delete_count: usize = table_changes.values().map(|(_, deletes)| deletes).sum();

    // Log details
    println!(
        "Received {} updates and {} deletes across {} tables",
        update_count,
        delete_count,
        table_changes.len()
    );

    for (table, (updates, deletes)) in &table_changes {
        if *updates > 0 || *deletes > 0 {
            println!(
                "  Table '{}': {} updates, {} deletes",
                table, updates, deletes
            );
        }
    }

    // Return empty response
    Ok(Json(SyncDatabaseChangeSet::new()))
}

// helper to convert RawRecord to LocalDBEntry
fn record_to_local_db_entry(record: &RawRecord) -> LocalDBEntry {
    let current_timestamp = timestamp();
    let mut data = HashMap::new();

    // Copy all existing data fields
    for (key, value) in &record.data {
        data.insert(key.clone(), value.clone());
    }

    // Ensure id, created_at, and updated_at are also present in the data field
    data.insert("id".to_string(), serde_json::Value::String(record.id.clone()));
    data.insert("created_at".to_string(), serde_json::Value::Number(
        serde_json::Number::from(record.created_at)
    ));
    data.insert("updated_at".to_string(), serde_json::Value::Number(
        serde_json::Number::from(record.updated_at)
    ));

    let local_db_entry = LocalDBEntry {
        id: record.id.clone(),
        created_at: record.created_at,
        updated_at: record.updated_at,
        local_server_created_at: current_timestamp,
        local_server_last_modified: current_timestamp,
        data,
    };
    println!("LocalDBEntry: {:#?}", local_db_entry);
    println!("Record: {:#?}", record);
    local_db_entry
}

// Get current timestamp in milliseconds (Unix epoch)
fn timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

// #[tauri::command]
// async fn generate_self_signed_cert<'a>(
//     app_handle: tauri::AppHandle,
// ) -> Result<String, String> {
//     // Get app directory for certificate storage
//     let app_dir = app_handle.path_resolver()
//         .app_dir()
//         .ok_or_else(|| "Failed to get app directory".to_string())?;
    
//     // Create certs directory if it doesn't exist
//     let cert_dir = app_dir.join("certs");
//     if !cert_dir.exists() {
//         tokio::fs::create_dir_all(&cert_dir)
//             .await
//             .map_err(|e| format!("Failed to create certs directory: {}", e))?;
//     }
    
//     let cert_path = cert_dir.join("server.crt");
//     let key_path = cert_dir.join("server.key");
    
//     // Check if certificates already exist
//     if cert_path.exists() && key_path.exists() {
//         return Ok("Certificates already exist".to_string());
//     }
    
//     // Generate self-signed certificate using openssl command
//     // This is for development/testing purposes only
//     let output = std::process::Command::new("openssl")
//         .args([
//             "req", "-x509", "-newkey", "rsa:4096", 
//             "-keyout", key_path.to_str().unwrap(), 
//             "-out", cert_path.to_str().unwrap(),
//             "-days", "365", "-nodes", "-subj", "/CN=localhost"
//         ])
//         .output()
//         .map_err(|e| format!("Failed to execute openssl command: {}", e))?;
    
//     if !output.status.success() {
//         let error = String::from_utf8_lossy(&output.stderr);
//         return Err(format!("OpenSSL command failed: {}", error));
//     }
    
//     Ok(format!(
//         "Generated self-signed certificates at:\n- Certificate: {:?}\n- Key: {:?}",
//         cert_path, key_path
//     ))
// }


// Function to load SSL certificates for HTTPS
// async fn load_certificates(cert_path: &str, key_path: &str) -> std::result::Result<TlsConfig, String> {
//     // Load TLS certificate and key files
//     let cert_file = tokio::fs::read(cert_path)
//         .await
//         .map_err(|e| format!("Failed to read certificate file: {}", e))?;
    
//     let key_file = tokio::fs::read(key_path)
//         .await
//         .map_err(|e| format!("Failed to read key file: {}", e))?;
    
//     // Create TLS configuration
//     TlsConfig::new()
//         .cert(cert_file)
//         .key(key_file)
//         .map_err(|e| format!("Failed to create TLS config: {}", e))
// }


// TODO: cycle the certificate generation
async fn start_server(
    ip_address: IpAddr,
    shutdown_token: Arc<tokio::sync::Notify>,
) -> Result<(), String> {
    // init_db().await;

    let app = Route::new()
        .at("/", get(index_route))
        .at("/hello/:name", get(hello))
        .at("/sync", get(get_sync).post(post_sync));

    // Bind to 0.0.0.0 to accept connections from all interfaces
    let bind_address = "0.0.0.0:3000";
    println!("Starting server on all interfaces at port 3000");
    println!("Server should be accessible at: http://{}:3000", ip_address);

    let exe_path = std::env::current_exe().unwrap();
    let mut exe_path = exe_path.parent().unwrap().to_path_buf();
    exe_path.push("certs");

    // Create certs directory if it doesn't exist
    if !exe_path.exists() {
        std::fs::create_dir_all(&exe_path)
            .map_err(|e| format!("Failed to create certs directory: {}", e))?;
    }

    let cert_path = exe_path.join("server.crt");
    let key_path = exe_path.join("server.key");

    // Generate certificates if they don't exist
    if !cert_path.exists() || !key_path.exists() {
        println!("Generating self-signed certificates...");
        // Generate self-signed certificate with localhost and IP as subject alternative names
        let subject_alt_names = vec![
            ip_address.to_string(),
            "localhost".to_string()
        ];
        
        let certified_key = generate_simple_self_signed(subject_alt_names)
            .map_err(|e| format!("Failed to generate certificate: {}", e))?;
        
        // Write certificate and key to files
        std::fs::write(&cert_path, certified_key.cert.pem())
            .map_err(|e| format!("Failed to write certificate file: {}", e))?;
        
        std::fs::write(&key_path, certified_key.key_pair.serialize_pem())
            .map_err(|e| format!("Failed to write key file: {}", e))?;
        
        println!("Generated certificates at: {:?} and {:?}", cert_path, key_path);
    }

    // Create a future that completes when the shutdown signal is received
    let shutdown_future = shutdown_token.notified();

    // Run the server with the appropriate listener based on certificate availability
    // Self signed certificates are causing trouble with ios and android security rules. Leaving out until a better solution comes up
    // TODO: Consider an alternative where where we just encrypt and decrypt the data on our own.
    let _listener = if cert_path.exists() && key_path.exists() {
        println!("Starting HTTPS server on all interfaces at port 3000");
        println!("Server should be accessible at: https://{}", ip_address);
        
        // Load certificates
        let cert_file = tokio::fs::read(cert_path)
            .await
            .map_err(|e| format!("Failed to read certificate file: {}", e))?;
        
        let key_file = tokio::fs::read(key_path)
            .await
            .map_err(|e| format!("Failed to read key file: {}", e))?;
        
        // Create TLS listener
        TcpListener::bind(bind_address)
            .rustls(RustlsConfig::new().fallback(RustlsCertificate::new().key(key_file).cert(cert_file)))
    } else {
        println!("Starting HTTP server on all interfaces at port 3000");
        println!("Server should be accessible at: http://{}", ip_address);
        // println!("HTTPS not available: certificates not found at {:?} and {:?}", cert_path, key_path);
        
        // Create plain TCP listener
        TcpListener::bind(bind_address).rustls(RustlsConfig::default())
    };

    // The listener without certificates - using this one until we can figure out how to get certificates signed by a central authority (CA)
    let no_cert_listener = TcpListener::bind(bind_address);
    
    // Run the server with the selected listener type
    let server_future = Server::new(no_cert_listener).run(app);

    // Race between the server and the shutdown signal
    match tokio::select! {
        result = server_future => result.map_err(|e| e.to_string()),
        _ = shutdown_future => {
            println!("Server shutdown requested");
            Ok(())
        }
    } {
        Ok(_) => Ok(()),
        Err(e) => {
            eprintln!("Server error: {}", e);
            Err(e)
        }
    }
}

async fn init_db() {
    let db_url = db::get_database();
    db::create(&db_url).await;

    db::migrate(&db_url).await;

    println!("Database created at: {}", db_url);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // TODO: this must be changed
    // let db_url = db::get_database();
    let db_url = "sqlite:hikma-health.db";
    // std::fs::write("hikma-health.db", b"").ok();


    // sqlx::sqlite::SqlitePool::mi(&db_url).await.unwrap();


    let migrations = HH_MIGRATIONS.iter().map(|m| Migration {
        version: m.version,
        description: m.description,
        sql: m.sql,
        kind: match m.kind {
            MigrationKind::Up => MigrationKind::Up,
            MigrationKind::Down => MigrationKind::Down,
        },
    }).collect();

    // Initialize with server state
    tauri::Builder::default()
        .plugin(
            tauri_plugin_sql::Builder::default()
                // Manually create the Vec<Migration> since Migration is not Clone
                .add_migrations(
                    &db_url,
                    migrations,
                )
                .build(),
        )
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .manage(ServerState::default())
        // .manage(DB.clone())
        .invoke_handler(tauri::generate_handler![
            greet,
            start_server_command,
            stop_server_command,
            get_server_status,
            // generate_self_signed_cert
        ])
        // .setup(|app| {
        //     // Generate self-signed certificate
            
        // })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// Simple immutable server state
#[derive(Default)]
struct ServerState {
    is_running: Arc<std::sync::atomic::AtomicBool>,
    address: Arc<parking_lot::RwLock<Option<String>>>,
    shutdown_token: Arc<tokio::sync::Notify>,
}

impl ServerState {
    fn default() -> Self {
        Self {
            is_running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            address: Arc::new(parking_lot::RwLock::new(None)),
            shutdown_token: Arc::new(tokio::sync::Notify::new()),
        }
    }
}

// Pure function to get server status
#[tauri::command]
fn get_server_status(state: tauri::State<ServerState>) -> Result<(bool, Option<String>), String> {
    let is_running = state.is_running.load(std::sync::atomic::Ordering::Relaxed);
    let address = state.address.read().clone();
    Ok((is_running, address))
}

// Start server command - more functional with less mutable state
#[tauri::command]
async fn start_server_command<'a>(state: tauri::State<'a, ServerState>) -> Result<String, String> {
    init_db().await;

    // Check if already running using atomic boolean
    if state.is_running.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("Server is already running".to_string());
    }

    // Get IP address
    local_ip()
        .map_err(|e| format!("Failed to get local IP: {}", e))
        .and_then(|ip_address| {
            // Start server in background
            let server_address = format!("http://{}:3000", ip_address);

            // Clone the state components for use in the async block
            let is_running = state.is_running.clone();
            let address = state.address.clone();
            let shutdown_token = state.shutdown_token.clone();

            // Spawn server and update state atomically
            tauri::async_runtime::spawn(async move {
                // Set running state before starting
                is_running.store(true, std::sync::atomic::Ordering::Relaxed);
                *address.write() = Some(server_address.clone());

                // Run server (this will block until server exits or shutdown is requested)
                let server_result = start_server(ip_address, shutdown_token).await;

                // Reset state when server exits
                is_running.store(false, std::sync::atomic::Ordering::Relaxed);
                *address.write() = None;

                // Log any errors
                if let Err(e) = server_result {
                    eprintln!("Server error: {}", e);
                }
            });

            Ok(format!("Server started at http://{}:3000", ip_address))
        })
}

// Stop server command that actually stops the server
#[tauri::command]
fn stop_server_command(state: tauri::State<ServerState>) -> Result<String, String> {
    // Check if server is running
    if !state.is_running.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("No server is running".to_string());
    }

    // Signal the server to shut down
    state.shutdown_token.notify_one();

    // Return success message
    Ok("Server shutdown initiated".to_string())
}
