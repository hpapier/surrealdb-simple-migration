# surrealdb-simple-migration

A simple library for surrealdb migration process.

## Usage

1. This library migrates files under the pattern `<file_number>(_<filename>).surql`.
Here some examples:
```
    path_to_dir/001.surql
    path_to_dir/002_create_users_table.surql
    path_to_dir/003_drop.surql
```

2. In code:
```
    let db_connection = ...;
    let migration_directory_path = "your/custom/path";

    // Here the code from the Library
    surrealdb_simple_migration::migrate(&db_connection, migration_directory_path).await;
```