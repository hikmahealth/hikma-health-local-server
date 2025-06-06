use std::env;
use std::io;
use std::path::PathBuf;

use sqlx::migrate::Migrator;
use sqlx::{migrate::MigrateDatabase, Sqlite, SqlitePool};

fn get_database_path() -> io::Result<PathBuf> {
    let mut exe = env::current_exe()?;
    // exe.set_file_name("./hikma-health.db");
    exe.set_file_name("sqlite:hikma-health.db");
    #[cfg(dev)]
    exe.set_file_name("sqlite:hikma-health.db");
    Ok(exe)
}

fn get_migrations_path() -> io::Result<PathBuf> {
    let mut exe = env::current_exe()?;
    exe.set_file_name("./migrations");
    #[cfg(dev)]
    exe.set_file_name("../../migrations");
    Ok(exe)
}

#[allow(dead_code)]
pub async fn create(db_url: &str) {
    if !Sqlite::database_exists(db_url).await.unwrap_or(false) {
        match Sqlite::create_database(db_url).await {
            Ok(_) => println!("Create db success"),
            Err(error) => panic!("error: {}", error),
        }
    } else {
        println!("Database already exists");
    }
}

pub async fn migrate(db_url: &str) -> Result<(), sqlx::Error> {
    let pool = SqlitePool::connect(db_url).await?;
    let migrations_path = get_migrations_path().unwrap();

    let m = Migrator::new(migrations_path).await?;
    m.run(&pool).await?;
    Ok(())
}

pub fn get_database() -> String {
    let db_url = match get_database_path() {
        Ok(path) => path.into_os_string().into_string().unwrap(),
        Err(e) => e.to_string(),
    };
    return db_url;
}
