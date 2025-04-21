use tauri_plugin_sql::{Migration, MigrationKind};

pub mod create_initial_tables;

pub fn get_migrations() -> Vec<&'static str> {
    vec![create_initial_tables::get_migration()]
}

pub const HH_MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    description: "create_initial_tables",
    sql: create_initial_tables::get_migration(),
    kind: MigrationKind::Up,
    // TODO: define the down migration
}];
