
// use std::io::Read;

use serde::Deserialize;

use surrealdb::{engine::remote::ws::Client, Surreal};
use tokio::{fs::{read_dir, File}, io::AsyncReadExt};

#[derive(Deserialize, PartialEq, Debug)]
pub struct Migration {
    filename: String,
    created_at: String,
}

impl PartialEq<String> for Migration {
    fn eq(&self, other: &String) -> bool {
        self.filename.to_string() == *other
    }
}

pub async fn migrate(db: &Surreal<Client>, migration_dir_path: &str) {
    let _ = setup_migration_table(db).await.unwrap();
    let _ = run_migration_files(db, migration_dir_path).await.unwrap();
}

async fn setup_migration_table(db: &Surreal<Client>) -> Result<(), surrealdb::Error> {
    let sql = r#"
        DEFINE TABLE migrations SCHEMAFULL;
        DEFINE FIELD filename ON TABLE migrations TYPE string;
        DEFINE FIELD created_at ON TABLE migrations TYPE datetime VALUE time::now();
    "#;

    let _ = db
        .query(sql)
        .await?
        .check()?;

    Ok(())
}

async fn run_migration_files(db: &Surreal<Client>, migration_dir_path: &str) -> Result<(), surrealdb::Error> {
    // Get the files already processed.
    let migrations = db
        .query("SELECT * FROM migrations;")
        .await?
        .check()?
        .take::<Vec<Migration>>(0)?;

    // Get the surql migration files to execute.
    let mut dir = read_dir(migration_dir_path).await.unwrap();
    let mut entries: Vec<String> = vec![];

    loop {
        let entry = dir.next_entry().await;
        match entry {
            Ok(entry) => match entry {
                Some(entry) => {
                    let filename = entry.path().to_str().unwrap().to_string();
                    // TODO: Ensure the file has this pattern: <number>_<purpose>.surql
                    if filename.contains(".surql") {
                        entries.push(filename);
                    }
                },
                None => break
            },
            Err(e) => println!("An error occured while reading the file: {}", e),
        }
    }

    entries.sort();

    // Process migration files.
    println!("{:?}", migrations);
    println!("{:?}", entries);

    for entry in entries {
        let migrated = migrations
            .iter()
            .any(|migration| migration == &entry);

        if migrated {
            println!("[V] File already migrated: {}", entry);
        } else {
            let mut migration_content: String = String::new();
            let result = File::open(&entry).await.unwrap().read_to_string(&mut migration_content).await;
            if let Err(err) = result {
                println!("An error occured while opening the migration file {}, ERROR: {}", entry, err);
            }

            let _ = db.query(migration_content).await?;
            // if let Err(err) = result {
            //     println!("An error occured while migrating {}, ERROR: {}", entry, err);
            // }

            let _ = db
                .query("CREATE migrations SET filename=$filename;")
                .bind(("filename", &entry))
                .await?
                .check()?;

            // if let Err(err) = result {
            //     println!("An error occured while creating migration in database for {}, ERROR: {}", entry, err);
            // }

            println!("[V] File successfuly migrated: {}", &entry);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs::create_dir_all;

    use surrealdb::{engine::remote::ws::Ws, Surreal};
    use tokio::{fs::File, io::AsyncWriteExt};

    #[tokio::test]
    async fn it_migrates_migrations_files() {
        // Setup database.
        let db = Surreal::new::<Ws>("0.0.0.0:8000").await.unwrap();

        db
            .use_ns("env")
            .use_db("dev")
            .await
            .expect("Failed to use namespace 'env' with database 'dev'.");

        // 1. When migration files fit the required pattern, it should process them.
        // Arrange - Create fake migration files.
        let migration_dir_path = "migrations";

        let _ = create_dir_all(migration_dir_path).expect("Failed to create directory for migration files.");
        let mut file1 = File::create(migration_dir_path.to_owned() + "/001_create_user_table.surql").await.unwrap();
        file1.write_all(b"
            DEFINE TABLE user SCHEMAFULL;
            DEFINE FIELD name ON TABLE user TYPE string;
            DEFINE FIELD email ON TABLE users TYPE string;
            DEFINE FIELD created_at ON TABLE users TYPE datetime VALUE time::now();
        ").await.unwrap();

        let mut file2 = File::create(migration_dir_path.to_owned() + "/002_create_post_table.surql").await.unwrap();
        file2.write_all(b"
            DEFINE TABLE post SCHEMAFULL;
            DEFINE FIELD title ON TABLE user TYPE string;
            DEFINE FIELD content ON TABLE users TYPE string;
            DEFINE FIELD created_at ON TABLE users TYPE datetime VALUE time::now();
        ").await.unwrap();

        let mut file3 = File::create(migration_dir_path.to_owned() + "/003_create_comment_table.surql").await.unwrap();
        file3.write_all(b"
            DEFINE TABLE comment SCHEMAFULL;
            DEFINE FIELD content ON TABLE users TYPE string;
            DEFINE FIELD created_at ON TABLE users TYPE datetime VALUE time::now();
        ").await.unwrap();

        // Act - Run the migration.
        super::migrate(&db, migration_dir_path).await;

        // CLEANUP
        tokio::fs::remove_dir_all(migration_dir_path)
            .await
            .expect("Failed to remove directory for migration files.");
    }
}