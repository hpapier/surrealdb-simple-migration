extern crate chrono;

use std::fmt;
use chrono::prelude::*;

use regex::Regex;
use serde::Deserialize;

use surrealdb::{engine::remote::ws::Client, Surreal};
use tokio::{fs::{read_dir, File}, io::AsyncReadExt};

#[derive(Deserialize, PartialEq, Debug, Clone)]
pub struct Migration {
    filename: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug)]
pub enum Error {
    IO(std::io::Error),
    Surreal(surrealdb::Error),
    ForbiddenUpdate(String),
    ForbiddenRemoval(String),
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::IO(err)
    }
}

impl From<surrealdb::Error> for Error {
    fn from(err: surrealdb::Error) -> Self {
        Error::Surreal(err)
    }
}

impl PartialEq<String> for Migration {
    fn eq(&self, other: &String) -> bool {
        self.filename.to_string() == *other
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Error::IO(ref err) => write!(f, "IO error: {}", err),
            Error::Surreal(ref err) => write!(f, "Surreal error: {}", err),
            Error::ForbiddenUpdate(ref err) => write!(f, "Forbidden update: {}", err),
            Error::ForbiddenRemoval(ref err) => write!(f, "Forbidden removal: {}", err),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self {
            Error::IO(ref err) => Some(err),
            Error::Surreal(ref err) => Some(err),
            Error::ForbiddenUpdate(_) => None,
            Error::ForbiddenRemoval(_) => None,
        }
    }

}

pub async fn migrate(db: &Surreal<Client>, migration_dir_path: &str) -> Result<(), Error> {
    setup_migration_table(db).await?;
    run_migration_files(db, migration_dir_path).await?;

    Ok(())
}

async fn setup_migration_table(db: &Surreal<Client>) -> Result<(), surrealdb::Error> {
    let sql = r#"
        DEFINE TABLE IF NOT EXISTS migrations SCHEMAFULL;
        DEFINE FIELD IF NOT EXISTS filename ON TABLE migrations TYPE string;
        DEFINE FIELD IF NOT EXISTS created_at ON TABLE migrations TYPE datetime VALUE time::now();
    "#;

    let _ = db
        .query(sql)
        .await?
        .check()?;

    Ok(())
}

async fn run_migration_files(db: &Surreal<Client>, migration_dir_path: &str) -> Result<(), Error> {
    // Get the files already processed.
    let migrations = db
        .query("SELECT * FROM migrations ORDER BY created_at ASC;")
        .await?
        .check()?
        .take::<Vec<Migration>>(0)?;
    let mut remaining_migrations: Vec<Migration> = migrations.clone();

    println!("Migrated files: {:#?}", migrations);

    // Get the surql migration files to execute.
    let mut dir = read_dir(migration_dir_path).await?;
    let mut entries: Vec<String> = vec![];

    // Filter the files that fit the migration pattern.
    while let Some(dir_entry) = dir.next_entry().await? {
        let filename = dir_entry.path().to_str().unwrap().to_string().replace((migration_dir_path.to_owned() + "/").as_str(), "");
        let pattern = r"^[0-9]+[a-zA-Z_0-9]{0,}\.surql$";
        let regex = Regex::new(&pattern).expect("Failed to build the regexp");
        if regex.is_match(&filename) {
            entries.push(filename);
        }
    }

    // Sort the entries (by their number prefix).
    entries.sort(); // TODO: Check how the strings are sorted.

    // Process migration files.
    println!("Migration files: {:#?}", entries);

    let last_migration = migrations.last();

    // Checker - check for forbidden updates and removals.
    for entry in entries {
        // Get the file descriptor.
        let mut file = File::open(migration_dir_path.to_owned() + "/" + &entry).await?;

        // Check if the file has already been migrated.
        let migrated = migrations
            .iter()
            .any(|migration: &Migration| migration == &entry);

        // If migrated, check that the last update date is anterior to the created_at.
        if migrated {
            let updated_at: DateTime<Utc> = File::metadata(&file)
                .await?
                .modified()?
                .into();

            // Ensure the file has not been updated after the last migration.
            if updated_at > last_migration.unwrap().created_at {
                println!("[X] Forbidden: The migration file '{}' has been updated after the last migration.", entry);
                return Err(
                    Error::ForbiddenUpdate(
                        format!("Forbidden: The migration file '{}' has been updated after the last migration.", entry)
                    )
                );
            }

            println!("[V] File already migrated: {}", entry);
        } else {
            // TODO: Check that the new migration file appears after the last migration file.
            let mut migration_content: String = String::new();
            file.read_to_string(&mut migration_content).await?;

            // When the last migration file is created after the current file, it should fail.
            if last_migration != None && last_migration.unwrap().created_at > DateTime::<Utc>::from(File::metadata(&file).await?.modified()?) {
                println!("[X] The migration file '{}' appears before the last migration file '{}'.", &entry, last_migration.unwrap().filename);

                return Err(
                    Error::ForbiddenUpdate(
                        format!("The migration file '{}' appears before the last migration file '{}'.", &entry, last_migration.unwrap().filename)
                    )
                );
            }

            // Migrate the file.
            let _ = db.query(migration_content).await?;
            let _ = db
                .query("CREATE migrations SET filename=$filename;")
                .bind(("filename", entry.clone()))
                .await?
                .check()?;

            println!("[V] File successfuly migrated: {}", &entry);
        }

        // Update the migrations list.
        let position = remaining_migrations.iter().position(|migration| { *migration.filename == entry });
        if let Some(pos) = position {
            remaining_migrations.remove(pos);
        }
    }

    if remaining_migrations.len() > 0 {
        println!("[X] Some migration files are missing - migrations failed: {:?}", remaining_migrations);
        return Err(
            Error::ForbiddenRemoval(
                format!("Some migration files are missing - migrations failed: {:?}", remaining_migrations)
            )
        )
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs::create_dir_all;

    use surrealdb::{engine::remote::ws::Ws, opt::auth::Root, Surreal};
    use tokio::{fs::File, io::AsyncWriteExt};

    async fn clean_up() {
        let db = Surreal::new::<Ws>("0.0.0.0:8000").await.unwrap();

        db.signin(Root {
            username: "root",
            password: "root"
        })
        .await
        .expect("Failed to sign in.");

        db
            .use_ns("env")
            .use_db("ssm_test")
            .await
            .expect("Failed to use namespace 'env' with database 'dev'.");

        let _ = tokio::fs::remove_dir_all("test/migrations").await;
        let _ = db.query("DELETE migrations;").await.expect("Failed to delete migrations table.");
    }

    #[tokio::test]
    async fn it_migrates_migration_files() {
        // Cleanup
        clean_up().await;

        // Setup database.
        let db = Surreal::new::<Ws>("0.0.0.0:8000").await.unwrap();

        db.signin(Root {
            username: "root",
            password: "root"
        })
        .await
        .expect("Failed to sign in.");

        db
            .use_ns("env")
            .use_db("ssm_test")
            .await
            .expect("Failed to use namespace 'env' with database 'dev'.");

        // 1. When migration files fit the required pattern, it should process them.
        // Arrange - Create fake migration files.
        let migration_dir_path = "test/migrations";

        let _ = create_dir_all(migration_dir_path).expect("Failed to create directory for migration files.");
        let mut file1 = File::create(migration_dir_path.to_owned() + "/001_create_user_table.surql").await.unwrap();
        file1.write_all(b"
            DEFINE TABLE users SCHEMAFULL;
            DEFINE FIELD name ON TABLE user TYPE string;
            DEFINE FIELD email ON TABLE users TYPE string;
            DEFINE FIELD created_at ON TABLE users TYPE datetime VALUE time::now();
        ").await.unwrap();

        let mut file2 = File::create(migration_dir_path.to_owned() + "/002_create_post_table.surql").await.unwrap();
        file2.write_all(b"
            DEFINE TABLE posts SCHEMAFULL;
            DEFINE FIELD title ON TABLE posts TYPE string;
            DEFINE FIELD content ON TABLE posts TYPE string;
            DEFINE FIELD created_at ON TABLE posts TYPE datetime VALUE time::now();
        ").await.unwrap();

        let mut file3 = File::create(migration_dir_path.to_owned() + "/003_create_comment_table.surql").await.unwrap();
        file3.write_all(b"
            DEFINE TABLE comments SCHEMAFULL;
            DEFINE FIELD content ON TABLE comments TYPE string;
            DEFINE FIELD created_at ON TABLE comments TYPE datetime VALUE time::now();
        ").await.unwrap();

        let mut file4 = File::create(migration_dir_path.to_owned() + "/004_i18n_table.surql").await.unwrap();
        file4.write_all(b"
            DEFINE TABLE i18n SCHEMAFULL;
            DEFINE FIELD locale ON TABLE i18n TYPE string;
            DEFINE FIELD text ON TABLE i18n TYPE string;
        ").await.unwrap();

        // Act - Run the migration.
        let result = super::migrate(&db, migration_dir_path).await;

        // Assert
        assert!(result.is_ok());

        // 2. When migration files are already processed, it should skip them.
        // Act - Run the migration again.
        let result = super::migrate(&db, migration_dir_path).await;

        // Assert
        assert!(result.is_ok());

        // 3. When new migration files are added, it should process them.
        // Arrange - Add a new migration file.
        let mut file5 = File::create(migration_dir_path.to_owned() + "/005_create_likes_table.surql").await.unwrap();
        file5.write_all(b"
            DEFINE TABLE likes SCHEMAFULL;
            DEFINE FIELD user_id ON TABLE likes TYPE record;
            DEFINE FIELD post_id ON TABLE likes TYPE string;
            DEFINE FIELD created_at ON TABLE likes TYPE datetime VALUE time::now();
        ").await.unwrap();

        // Act
        let result = super::migrate(&db, migration_dir_path).await;

        // Assert
        assert!(result.is_ok());

        // 4. When migration files are updated, it should fail.
        // Arrange - Update the migration files.
        file1.write(b"
            DEFINE FIELD updated_at ON TABLE users TYPE datetime VALUE time::now();
        ").await.unwrap();

        // Act - Run the migration again.
        let res = super::migrate(&db, migration_dir_path).await;

        // Assert
        assert!(res.is_err());

        // 5. When a migrated file is removed, it should return an error.
        // Arrange - Reset the migrations, migrate the files again and remove one file.
        let _ = db.query("DELETE migrations;").await;
        super::migrate(&db, migration_dir_path).await.expect("Failed to migrate the files.");
        tokio::fs::remove_file(migration_dir_path.to_owned() + "/001_create_user_table.surql").await.unwrap();

        // Act
        let res = super::migrate(&db, migration_dir_path).await;

        // Assert
        assert!(res.is_err());

        // CLEANUP
        clean_up().await;

        // data cleaning
        db.query("REMOVE TABLE migrations;").await.unwrap();
        db.query("REMOVE TABLE users;").await.unwrap();
        db.query("REMOVE TABLE posts;").await.unwrap();
        db.query("REMOVE TABLE comments;").await.unwrap();
        db.query("REMOVE TABLE likes;").await.unwrap();
    }
}