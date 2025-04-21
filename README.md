# Hikma Health Local Server

A local server and desktop app for hikma health.

This application runs on a computer devices and provides a local server for devices that share the same network, preferrable over wifi.

The mobile applications (running on android and ios and powered by react-native) connect to this local server to fetch data and upload data. This allows for local sync without the need to connect to the internet.

The local server will then connect to the remote server to sync data and upload data, and pull any new data.

## Migrations

sqlx migrate add -r <name>

### Local Database Schema

The data fields are stored in jsonb format. This allows for flexibility in the data structure and makes it easier to add new fields in the future. Since the cloud server and the mobile app database have custom strongly defined fields, and we want to use the local server simply as a transit server, this flexible approach seems easiest to work with.

Note: with "modern" SQLITE (3.9.0+) you can use json1 extension to deal with json data in columns

Tables:

- patients
- events
- visits
- clinics
- users
- event_forms
- registration_forms
- patient_additional_attributes
- appointments
- prescriptions

#### Patients Table

Fields:

- id uuid NOT NULL PRIMARY KEY
- data jsonb NOT NULL
- local_server_created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP
- local_server_last_modified timestamp with time zone DEFAULT CURRENT_TIMESTAMP
- local_server_deleted_at timestamp with time zone DEFAULT NULL

#### Events Table

Fields:

- id uuid NOT NULL PRIMARY KEY
- data jsonb NOT NULL
- local_server_created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP
- local_server_last_modified timestamp with time zone DEFAULT CURRENT_TIMESTAMP
- local_server_deleted_at timestamp with time zone DEFAULT NULL

#### Visits Table

Fields:

- id uuid NOT NULL PRIMARY KEY
- data jsonb NOT NULL
- local_server_created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP
- local_server_last_modified timestamp with time zone DEFAULT CURRENT_TIMESTAMP
- local_server_deleted_at timestamp with time zone DEFAULT NULL

#### Clinics Table

Fields:

- id uuid NOT NULL PRIMARY KEY
- data jsonb NOT NULL
- local_server_created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP
- local_server_last_modified timestamp with time zone DEFAULT CURRENT_TIMESTAMP
- local_server_deleted_at timestamp with time zone DEFAULT NULL

#### Users Table

Fields:

- id uuid NOT NULL PRIMARY KEY
- data jsonb NOT NULL
- local_server_created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP
- local_server_last_modified timestamp with time zone DEFAULT CURRENT_TIMESTAMP
- local_server_deleted_at timestamp with time zone DEFAULT NULL

#### Event Forms Table

Fields:

- id uuid NOT NULL PRIMARY KEY
- data jsonb NOT NULL
- local_server_created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP
- local_server_last_modified timestamp with time zone DEFAULT CURRENT_TIMESTAMP
- local_server_deleted_at timestamp with time zone DEFAULT NULL

#### Registration Forms Table

Fields:

- id uuid NOT NULL PRIMARY KEY
- data jsonb NOT NULL
- local_server_created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP
- local_server_last_modified timestamp with time zone DEFAULT CURRENT_TIMESTAMP
- local_server_deleted_at timestamp with time zone DEFAULT NULL

#### Patient Additional Attributes Table

Fields:

- id uuid NOT NULL PRIMARY KEY
- data jsonb NOT NULL
- local_server_created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP
- local_server_last_modified timestamp with time zone DEFAULT CURRENT_TIMESTAMP
- local_server_deleted_at timestamp with time zone DEFAULT NULL

#### Appointments Table

Fields:

- id uuid NOT NULL PRIMARY KEY
- data jsonb NOT NULL
- local_server_created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP
- local_server_last_modified timestamp with time zone DEFAULT CURRENT_TIMESTAMP
- local_server_deleted_at timestamp with time zone DEFAULT NULL

#### Prescriptions Table

Fields:

- id uuid NOT NULL PRIMARY KEY
- data jsonb NOT NULL
- local_server_created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP
- local_server_last_modified timestamp with time zone DEFAULT CURRENT_TIMESTAMP
- local_server_deleted_at timestamp with time zone DEFAULT NULL
