use surrealdb::{engine::remote::ws::Ws, Surreal};
use surrealdb_simple_migration::migrate;

#[tokio::main]
async fn main() {
    let db = Surreal::new::<Ws>("0.0.0.0:8000").await.unwrap();
    
    db
        .use_ns("env")
        .use_db("dev")
        .await
        .expect("Failed to use namespace 'env' with database 'dev'.");
    
    migrate(&db, "migrations/").await;
}
