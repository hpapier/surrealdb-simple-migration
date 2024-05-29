use std::env;

use surrealdb::{engine::remote::ws::Ws, Surreal};
use surrealdb_simple_migration::migrate;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "surrealdb-simple-migration",
    version,
    about = "A simple CLI to apply or remove migrations to a SurrealDB instance."
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// The host of the SurrealDB instance. (default: "http://localhost:8000")
    #[arg(short = 'H', long, global = true)]
    host: Option<String>,

    /// The path for the migration files. (default: "./")
    #[arg(short, long, global = true)]
    path: Option<String>,

    /// The namespace used on the surrealdb instance. (default: "default")
    #[arg(short, long, global = true)]
    namespace: Option<String>,

    /// The database used on the surrealdb instance. (default: "dev")
    #[arg(short, long, global = true)]
    database: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Apply all migrations.
    Apply,

    /// Remove all migrations from migrations table and delete the database in order to remove the effect of the migrations.
    Reset,
}

#[tokio::main]
async fn main() {
    let args = Cli::parse();

    println!("{:#?}", args);

    let host = args
        .host
        .unwrap_or_else(
            || env::var("SSM_HOST")
                .unwrap_or_else(|_| "http://localhost:8000".to_string())
        );

    let path = args
        .path
        .unwrap_or_else(
            || env::var("SSM_PATH")
                .unwrap_or_else(|_| "./".to_string())
        );

    let namespace = args
        .namespace
        .unwrap_or_else(
            || env::var("SSM_NAMESPACE")
                .unwrap_or_else(|_| "default".to_string())
        );

    let database = args
        .database
        .unwrap_or_else(
            || env::var("SSM_DATABASE")
                .unwrap_or_else(|_| "dev".to_string())
        );

    let db = Surreal::new::<Ws>(host).await.unwrap();
    
    db
        .use_ns(&namespace)
        .use_db(&database)
        .await
        .expect(format!("Failed to use namespace {} with database {}.", namespace, database).as_str());
    
    match args.command {
        Commands::Apply => {
            let result = migrate(&db, path.as_str()).await;
            match result {
                Ok(_) => (),
                Err(e) => eprintln!("Failed to apply migrations: {:?}", e),
            }
        },
        Commands::Reset => {
            let result = db
                .query("DELETE FROM migrations")
                .await;

            if let Err(e) = result {
                return eprintln!("Failed to reset migrations table: {:?}", e);
            }

            let result = db
                .query(format!("REMOVE DATABASE {};", &database).as_str())
                .await;

            if let Err(e) = result {
                return eprintln!("Failed to remove database: {:?}", e);
            }

            return println!("Migrations table and database successfully removed.");
        }
    }

    ()
}
