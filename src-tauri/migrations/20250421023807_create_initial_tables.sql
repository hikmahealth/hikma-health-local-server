CREATE TABLE IF NOT EXISTS patients (
        id TEXT PRIMARY KEY NOT NULL, 
        data JSON NOT NULL,
        local_server_created_at INTEGER DEFAULT (strftime('%s', 'now')),
        local_server_last_modified INTEGER DEFAULT (strftime('%s', 'now')),
        local_server_deleted_at TIMESTAMP DEFAULT NULL
    );

CREATE TABLE IF NOT EXISTS events (
        id TEXT PRIMARY KEY NOT NULL, 
        data JSON NOT NULL, 
        local_server_created_at INTEGER DEFAULT (strftime('%s', 'now')), 
        local_server_last_modified INTEGER DEFAULT (strftime('%s', 'now')), 
        local_server_deleted_at TIMESTAMP DEFAULT NULL
    );

CREATE TABLE IF NOT EXISTS visits (
        id TEXT PRIMARY KEY NOT NULL, 
        data JSON NOT NULL, 
        local_server_created_at INTEGER DEFAULT (strftime('%s', 'now')), 
        local_server_last_modified INTEGER DEFAULT (strftime('%s', 'now')), 
        local_server_deleted_at TIMESTAMP DEFAULT NULL
    );

CREATE TABLE IF NOT EXISTS clinics (
        id TEXT PRIMARY KEY NOT NULL, 
        data JSON NOT NULL, 
        local_server_created_at INTEGER DEFAULT (strftime('%s', 'now')), 
        local_server_last_modified INTEGER DEFAULT (strftime('%s', 'now')), 
        local_server_deleted_at TIMESTAMP DEFAULT NULL
    );

CREATE TABLE IF NOT EXISTS users (
        id TEXT PRIMARY KEY NOT NULL, 
        data JSON NOT NULL, 
        local_server_created_at INTEGER DEFAULT (strftime('%s', 'now')), 
        local_server_last_modified INTEGER DEFAULT (strftime('%s', 'now')), 
        local_server_deleted_at TIMESTAMP DEFAULT NULL
    );

CREATE TABLE IF NOT EXISTS event_forms (
        id TEXT PRIMARY KEY NOT NULL, 
        data JSON NOT NULL, 
        local_server_created_at INTEGER DEFAULT (strftime('%s', 'now')), 
        local_server_last_modified INTEGER DEFAULT (strftime('%s', 'now')), 
        local_server_deleted_at TIMESTAMP DEFAULT NULL
    );

CREATE TABLE IF NOT EXISTS registration_forms (
        id TEXT PRIMARY KEY NOT NULL, 
        data JSON NOT NULL, 
        local_server_created_at INTEGER DEFAULT (strftime('%s', 'now')), 
        local_server_last_modified INTEGER DEFAULT (strftime('%s', 'now')), 
        local_server_deleted_at TIMESTAMP DEFAULT NULL
    );

CREATE TABLE IF NOT EXISTS patient_additional_attributes (
        id TEXT PRIMARY KEY NOT NULL, 
        data JSON NOT NULL, 
        local_server_created_at INTEGER DEFAULT (strftime('%s', 'now')), 
        local_server_last_modified INTEGER DEFAULT (strftime('%s', 'now')), 
        local_server_deleted_at TIMESTAMP DEFAULT NULL
    );

CREATE TABLE IF NOT EXISTS appointments (
        id TEXT PRIMARY KEY NOT NULL, 
        data JSON NOT NULL, 
        local_server_created_at INTEGER DEFAULT (strftime('%s', 'now')), 
        local_server_last_modified INTEGER DEFAULT (strftime('%s', 'now')), 
        local_server_deleted_at TIMESTAMP DEFAULT NULL
    );

CREATE TABLE IF NOT EXISTS prescriptions (
        id TEXT PRIMARY KEY NOT NULL, 
        data JSON NOT NULL, 
        local_server_created_at INTEGER DEFAULT (strftime('%s', 'now')), 
        local_server_last_modified INTEGER DEFAULT (strftime('%s', 'now')), 
        local_server_deleted_at TIMESTAMP DEFAULT NULL
    );
