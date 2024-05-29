# surrealdb-simple-migration

A simple library for surrealdb migration process.

## Usage - Library

1. This library migrates files under the pattern `<file_number>(_<filename>).surql`.
Here some examples:
```shell
    path_to_dir/001.surql
    path_to_dir/002_create_users_table.surql
    path_to_dir/003_drop.surql
```

2. In code:
```rust
    let db_connection = ...;
    let migration_directory_path = "your/custom/path";

    // Here the code from the Library
    surrealdb_simple_migration::migrate(&db_connection, migration_directory_path).await;
```

## Usage - Command Line Interface

Install the package using `cargo install surrealdb-simple-migration`. It will automatically install the binary named `ssm` (short for `surrealdb-simple-migration`). Once installed, just run the command `ssm apply` to apply your migrations files. (default path for the directory of your migration files: `./`, default host address for you surrealdb instance `http://localhost:8000`).

The default namespace and database used on the surrealdb instance are `default` and `dev`.

If you want to reset your migrations use `ssm reset`.

### CLI Configuration

You can config the CLI to use either your environment variables or pass the desired information as options.

- `SSM_HOST` OR `-H your_host_address` in the CLI : Setup the host address (default `http://localhost:8000`).
- `SSM_PATH` OR `-p your/migration/files/path/` in the CLI : Setup the path used to run the migrations against (default to `./`).
- `SSM_NAMESPACE` OR `-n the_database_namespace` in the CLI : Setup the namespace used to run the migrations against (default to `default`).
- `SSM_DB_NAME` OR `-n the_database_namespace` in the CLI : Setup the database used to run the migrations against (default to `dev`).