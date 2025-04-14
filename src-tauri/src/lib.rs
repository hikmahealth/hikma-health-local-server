use local_ip_address::local_ip;
use once_cell::sync::Lazy;
use poem::{
    get, handler,
    listener::TcpListener,
    post,
    web::{Json, Path, Query},
    Error, IntoResponse, Result, Route, Server,
};
use polodb_core::{
    bson::{doc, Document},
    CollectionT, Database,
};
use serde::{Deserialize, Serialize};
use serde_json;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{collections::HashMap, net::IpAddr};
use tauri_plugin_fs::FsExt;

// Lazily initialize the database instance
static DB: Lazy<Database> = Lazy::new(|| {
    println!("Initializing database connection...");

    // Use a relative path for simplicity for now
    let db_path = "hikma-health.db";
    // Open the database
    Database::open_path(db_path).expect("Failed to open database")
});

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
    pub deleted: Vec<RawRecord>,
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

#[handler]
fn get_sync(Query(params): Query<HashMap<String, String>>) -> Result<impl IntoResponse> {
    // Extract lastPulledAt parameter using functional approach
    params
        .get("lastPulledAt")
        .map(|timestamp| println!("GET sync request with lastPulledAt: {}", timestamp))
        .unwrap_or_else(|| println!("GET sync request with no lastPulledAt parameter"));

    // Create empty response in a functional way
    let changes: SyncDatabaseChangeSet = SyncDatabaseChangeSet(HashMap::new());
    Ok(Json(changes))
}

#[handler]
fn post_sync(
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

    // Count the changes in each table
    let table_changes: HashMap<String, (usize, usize)> = body
        .0
        .iter()
        .map(|(table, changeset)| {
            println!("Table: {}", table);
            println!("Updated: {:#?}", changeset.updated);

            // Process updates using upserts for the table
            let updates_result = process_updates(table, &changeset.updated);
            if let Err(e) = updates_result {
                eprintln!("Error processing updates for table {}: {}", table, e);
            }

            // Process deletes
            // TODO: implement

            (
                table.clone(),
                (changeset.updated.len(), changeset.deleted.len()),
            )
        })
        .collect();

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

fn process_updates(table_name: &str, records: &[RawRecord]) -> Result<(), String> {
    if records.is_empty() {
        return Ok(());
    }

    // first we get or create the collection
    let collection = DB.collection::<Document>(table_name);

    // then we process the updates
    records.iter().try_for_each(|record| {
        // convert into doc for polodb
        let doc = record_to_document(record)?;

        // first find existing record by ID
        let filter = doc! { "id": &record.id };

        // upsert the record with update if exists but insert if not
        match collection.find_one(filter.clone()) {
            Ok(Some(_)) => {
                // Record exists, update it
                collection
                    .update_one(filter, doc)
                    .map_err(|e| format!("Failed to update record: {}", e))?;
            }
            Ok(None) => {
                // Record doesn't exist, insert it
                collection
                    .insert_one(doc)
                    .map_err(|e| format!("Failed to insert record: {}", e))?;
            }
            Err(e) => return Err(format!("Database error: {}", e)),
        }

        Ok(())
    })
}

// helper to convert RawRecord to Document
fn record_to_document(record: &RawRecord) -> Result<Document, String> {
    let current_timestamp = timestamp();
    let mut doc = doc! {
        "id": &record.id,
        "created_at": record.created_at,
        "updated_at": record.updated_at,
        "local_server_created_at": current_timestamp,
        "local_server_last_modified": current_timestamp
    };

    for (key, value) in &record.data {
        // Convert serde_json::Value to BSON value
        let bson_value = serde_json_to_bson_value(value)
            .map_err(|e| format!("Failed to convert field '{}': {}", key, e))?;

        doc.insert(key, bson_value);
    }

    Ok(doc)
}

// TODO: move into separate module
fn serde_json_to_bson_value(value: &serde_json::Value) -> Result<polodb_core::bson::Bson, String> {
    match value {
        serde_json::Value::Null => Ok(polodb_core::bson::Bson::Null),
        serde_json::Value::Bool(b) => Ok(polodb_core::bson::Bson::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(polodb_core::bson::Bson::Int64(i))
            } else if let Some(f) = n.as_f64() {
                Ok(polodb_core::bson::Bson::Double(f))
            } else {
                Err("Unsupported number type".to_string())
            }
        }
        serde_json::Value::String(s) => Ok(polodb_core::bson::Bson::String(s.clone())),
        serde_json::Value::Array(arr) => {
            let bson_array = arr
                .iter()
                .map(serde_json_to_bson_value)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(polodb_core::bson::Bson::Array(bson_array))
        }
        serde_json::Value::Object(obj) => {
            let mut doc = Document::new();
            for (k, v) in obj {
                doc.insert(k, serde_json_to_bson_value(v)?);
            }
            Ok(polodb_core::bson::Bson::Document(doc))
        }
    }
}

// Get current timestamp in milliseconds (Unix epoch)
fn timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

async fn start_server(
    ip_address: IpAddr,
    shutdown_token: Arc<tokio::sync::Notify>,
) -> Result<(), String> {
    let app = Route::new()
        .at("/", get(index_route))
        .at("/hello/:name", get(hello))
        .at("/sync", get(get_sync).post(post_sync));

    // Bind to 0.0.0.0 to accept connections from all interfaces
    let bind_address = "0.0.0.0:3000";
    println!("Starting server on all interfaces at port 3000");
    println!("Server should be accessible at: http://{}:3000", ip_address);

    // Create a future that completes when the shutdown signal is received
    let shutdown_future = shutdown_token.notified();

    // Run the server with graceful shutdown
    let server_future = Server::new(TcpListener::bind(bind_address)).run(app);

    // Get count of patients in the database
    let patients_collection = DB.collection::<Document>("patients");
    match patients_collection.count_documents() {
        Ok(count) => println!("Number of patients in database: {}", count),
        Err(e) => println!("Error counting patients: {}", e),
    }

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize with server state
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // allowed the given directory
            let scope = app.fs_scope();
            scope.allow_directory("/path/to/directory", false);

            Ok(())
        })
        .manage(ServerState::default())
        // .manage(DB.clone())
        .invoke_handler(tauri::generate_handler![
            greet,
            start_server_command,
            stop_server_command,
            get_server_status
        ])
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
fn start_server_command(state: tauri::State<ServerState>) -> Result<String, String> {
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
