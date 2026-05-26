use clap::builder::Str;
use native_tls::TlsConnector;
use postgres::{Client, Error as PostgresError, NoTls};
use postgres_native_tls::MakeTlsConnector;
use std::error::Error;

use crate::main;

// this contains all the info we need about the source db
// that needs to be applied to the target db
pub struct DbStructureResult {
    tables: Vec<Table>,
    extensions: Vec<String>,
}

pub struct Table {
    pub name: String,
    columns: Vec<Column>,
    primary_keys: Vec<PrimaryKey>,
    foreign_keys: Vec<ForeignKey>,
}

struct ColumnType {
    udt_name: String,         // raw postgres internal name type for column
    postgres_type_kind: char, // what 'type' of column (base 'b', enum 'e', domain 'd', etc...)
    enum_values: Option<Vec<String>>,
}

struct Column {
    pub name: String,
    pub data_type: ColumnType,
    pub is_nullable: bool,
}

struct ForeignKey {
    pub contraint_name: String,
    pub column_name: String,
    pub references_table: String,  // the table the fk points to
    pub references_column: String, // the column the fk points to (part of the table it points to)
}

struct PrimaryKey {
    name: String,
    column: String,
}

pub struct DbConfig {
    pub host: String,
    pub username: String,
    pub password: String,
    pub port: i32,
    pub database: String,
    pub require_tls: bool,
    pub schema: String,
}

impl DbConfig {
    fn build_postgres_conn_string(&self) -> String {
        if self.require_tls {
            format!(
                "postgres://{}:{}@{}:{}/{}?sslmode=require",
                self.username, self.password, self.host, self.port, self.database
            )
        } else {
            format!(
                "postgres://{}:{}@{}:{}/{}",
                self.username, self.password, self.host, self.port, self.database
            )
        }
    }

    pub fn create_client(&self) -> Result<Client, Box<dyn Error>> {
        if self.require_tls {
            let tls_connector = TlsConnector::builder().build()?;
            let tls = MakeTlsConnector::new(tls_connector.clone());
            let client = Client::connect(&self.build_postgres_conn_string(), tls)?;
            Ok(client)
        } else {
            let client = Client::connect(&self.build_postgres_conn_string(), NoTls)?;
            Ok(client)
        }
    }
}

pub fn get_db_structure(
    client: &mut Client,
    schema: &str,
) -> Result<DbStructureResult, Box<dyn Error>> {
    let mut tables: Vec<Table> = vec![];
    let mut extensions: Vec<String> = vec![];

    // get all the tables
    let table_results = client.query(
        "
        SELECT
            t.table_name
        FROM information_schema.tables t
        WHERE t.table_schema = $1
        AND t.table_type = 'BASE TABLE'
        ORDER BY t.table_name;
    ",
        &[&schema],
    )?;

    for t in &table_results {
        let table_name: &str = t.get("table_name");
        let mut exists: bool = false;

        // add table if not already exists
        for t in &tables {
            if t.name == table_name {
                exists = true;
                break;
            }
        }

        if !exists {
            tables.push(Table {
                name: table_name.trim().to_string(),
                columns: vec![],
                primary_keys: vec![],
                foreign_keys: vec![],
            })
        }
    }

    // get all columns
    let column_results = client.query(
        "
        SELECT
        c.table_name,
        c.column_name,

        CASE
            WHEN c.is_nullable = 'YES' THEN TRUE
            ELSE FALSE
        END AS is_nullable,

        c.udt_name,
        c.character_maximum_length,
        c.column_default,

        t.typtype AS postgres_type_kind,

        array_agg(e.enumlabel ORDER BY e.enumsortorder)
            FILTER (WHERE e.enumlabel IS NOT NULL) AS enum_values

    FROM information_schema.columns c

    LEFT JOIN pg_type t
        ON t.typname = c.udt_name

    LEFT JOIN pg_enum e
        ON e.enumtypid = t.oid

    WHERE c.table_schema = $1

    GROUP BY
        c.table_name,
        c.column_name,
        c.is_nullable,
        c.udt_name,
        c.character_maximum_length,
        c.column_default,
        c.ordinal_position,
        t.typtype

    ORDER BY c.table_name, c.ordinal_position;
    ",
        &[&schema],
    )?;

    for c in &column_results {
        let table_name: &str = c.get("table_name");
        let col_name: &str = c.get("column_name");
        let is_nullable: bool = c.get("is_nullable");
        let udt_name: &str = c.get("udt_name"); // udt stands for "user defined type"... the internal name postgres uses for a column type
        let character_max_length: Option<i32> = c.get("character_maximum_length");
        let column_default: Option<&str> = c.get("column_default");
        let postgres_type_kind: i8 = c.get("postgres_type_kind");
        let enum_values: Option<Vec<String>> = c.get("enum_values");

        for t in tables.iter_mut() {
            if t.name == table_name {
                t.columns.push(Column {
                    name: col_name.trim().to_string(),
                    data_type: ColumnType {
                        udt_name: udt_name.trim().to_string(),
                        postgres_type_kind: postgres_type_kind as u8 as char,
                        enum_values: enum_values,
                    },
                    is_nullable: is_nullable,
                });
                break;
            }
        }
    }

    // get the extensions of database
    let extension_results = client.query(
        "
        SELECT
            e.extname AS extension_name
        FROM pg_extension e
        ORDER BY e.extname;
    ",
        &[],
    )?;
    for e in &extension_results {
        let extension_name: &str = e.get("extension_name");
        extensions.push(extension_name.trim().to_string());
    }

    // get the primary keys
    let primary_key_results = client.query(
        "
        SELECT
        kcu.table_name,
        kcu.column_name,
        tc.constraint_name AS primary_key_name
    FROM information_schema.table_constraints tc
    JOIN information_schema.key_column_usage kcu
        ON tc.constraint_name = kcu.constraint_name
        AND tc.table_schema = kcu.table_schema
        AND tc.table_name = kcu.table_name
    WHERE tc.table_schema = $1
    AND tc.constraint_type = 'PRIMARY KEY'
    ORDER BY kcu.table_name, kcu.ordinal_position;
    ",
        &[&schema],
    )?;

    for pk in &primary_key_results {
        let table_name: &str = pk.get("table_name");
        let pk_name: &str = pk.get("primary_key_name");
        let pk_column: &str = pk.get("column_name");

        for t in tables.iter_mut() {
            if t.name == table_name {
                t.primary_keys.push(PrimaryKey {
                    name: pk_name.trim().to_string(),
                    column: pk_column.trim().to_string(),
                });
                break;
            }
        }
    }

    // get the foreign keys '
    let foreign_key_results = client.query(
        "
       SELECT
        kcu.constraint_name AS foreign_key_name,
        kcu.table_name AS table_name,
        kcu.column_name AS column_name,
        ccu.table_name AS references_table,
        ccu.column_name AS references_column
    FROM information_schema.table_constraints tc

    JOIN information_schema.key_column_usage kcu
        ON tc.constraint_name = kcu.constraint_name
        AND tc.table_schema = kcu.table_schema

    JOIN information_schema.constraint_column_usage ccu
        ON tc.constraint_name = ccu.constraint_name
        AND tc.table_schema = ccu.table_schema

    WHERE tc.constraint_type = 'FOREIGN KEY'
    AND tc.table_schema = $1

    ORDER BY
        kcu.table_name,
        kcu.constraint_name,
        kcu.ordinal_position;
    ",
        &[&schema],
    )?;

    for fk in &foreign_key_results {
        let table_name: &str = fk.get("table_name");
        let fk_contraint_name: &str = fk.get("foreign_key_name");
        let column_name: &str = fk.get("column_name");
        let references_table: &str = fk.get("references_table");
        let refernces_column: &str = fk.get("references_column");

        for t in tables.iter_mut() {
            if t.name == table_name {
                t.foreign_keys.push(ForeignKey {
                    contraint_name: fk_contraint_name.trim().to_string(),
                    column_name: column_name.trim().to_string(),
                    references_table: references_table.trim().to_string(),
                    references_column: refernces_column.trim().to_string(),
                });
                break;
            }
        }
    }

    Ok(DbStructureResult {
        tables: tables,
        extensions: extensions,
    })
}

pub fn generate_create_table_query(tables: &Vec<Table>) -> String {
    let mut create_queries = vec![];
    for t in tables {
        let mut col_queries: Vec<String> = vec![];
        for c in &t.columns {
            col_queries.push(format!("{} {}", c.name, c.data_type.udt_name))
        }

        let q: String = format!(
            "
            CREATE TABLE {} (
                {}
            )
        ",
            t.name,
            col_queries.join(",")
        );
        create_queries.push(q);
    }

    return create_queries.join(";\n");
}

pub fn remove_target_tables(
    client: &mut postgres::Transaction<'_>,
    schema: &str,
) -> Result<(), PostgresError> {
    // cant parameratize scheama into query, create custom string
    let query = format!(
        "
        DROP SCHEMA {} CASCADE;
        CREATE SCHEMA {};
    ",
        schema, schema
    );

    client.batch_execute(&query)?;

    Ok(())
}

pub fn has_tables(
    client: &mut postgres::Transaction<'_>,
    schema: &str,
) -> Result<i32, PostgresError> {
    // this is mainly used to check if the target db already has tables
    // returns the number of tables

    let q = client.query(
        "
    SELECT table_name
    FROM information_schema.tables
    WHERE table_schema = $1
    AND table_type = 'BASE TABLE'
    ORDER BY table_name;
    ",
        &[&schema],
    )?;

    Ok(q.len() as i32)
}

// this function will apply all the necessary extenstions, tables, and columns
// from the source database to the target database
pub fn create_target_schema(
    source_client: &mut Client,
    source_db_config: DbConfig,
    target_client: &mut postgres::Transaction<'_>,
) -> Result<(), Box<dyn Error>> {
    // first get the entire schema table structure from the source database
    // this will include all the important details needed for cloning a schema
    let db_structure = get_db_structure(source_client, &source_db_config.schema)?;

    // add the extensions to the db first before adding all the
    // tables and columns
    for ext in &db_structure.extensions {
        match target_client.execute(&format!("CREATE EXTENSION IF NOT EXISTS {};", ext), &[]) {
            Ok(_) => {}
            Err(e) => {
                println!(
                    "\nYou must install the {} extension on the target server before running.\n{:?}",
                    ext, e
                );
                return Err(Box::new(e));
            }
        }
    }

    // we need to check columns for any enum types and create those
    // before we attempt to create each table
    for t in &db_structure.tables {
        for c in &t.columns {
            if c.data_type.postgres_type_kind == 'e' {
                if let Some(enum_values) = &c.data_type.enum_values {
                    let mut quoted_enum_values: Vec<String> = vec![]; // need to add quotes around each enum value
                    for e in enum_values {
                        quoted_enum_values.push(format!("'{}'", e));
                    }

                    match target_client.execute(
                        &format!(
                            "CREATE TYPE {} as ENUM ({})",
                            c.data_type.udt_name,
                            quoted_enum_values.join(", ")
                        ),
                        &[],
                    ) {
                        Ok(_) => {}
                        Err(e) => {
                            println!(
                                "There was an error while creating the enum type '{}'\n{:?}",
                                c.data_type.udt_name, e
                            );
                            return Err(Box::new(e));
                        }
                    }
                }
            }
        }
    }

    // generate the CREATE query for each table
    // and exectute against the target database!
    target_client.batch_execute(&generate_create_table_query(&db_structure.tables))?;

    // once all the tables are created and ready, we need to add primary and foreign keys
    for table in &db_structure.tables {
        let primary_keys: &Vec<PrimaryKey> = &table.primary_keys;
        let foreign_keys: &Vec<ForeignKey> = &table.foreign_keys;

        // when adding primary keys we need to account for it being a
        // composite primary key
        let mut pk_columns: Vec<String> = vec![];
        for pk in primary_keys {
            pk_columns.push(pk.name.to_string());
        }

        // add the primary keys
        target_client.execute(
            &format!(
                "
            ALTER TABLE {}
            ADD CONSTRAINT {}
            PRIMARY KEY ({});
        ",
                table.name,
                "pk",
                pk_columns.join(", ")
            ),
            &[],
        )?;

        // add the foreign keys
        for fk in foreign_keys {
            target_client.execute(
                &format!(
                    "
            ALTER TABLE {}
            ADD CONSTRAINT {}
            FOREIGN KEY ({})
            REFERENCES {}({});
        ",
                    table.name,
                    fk.contraint_name,
                    fk.column_name,
                    fk.references_table,
                    fk.references_column
                ),
                &[],
            )?;
        }
    }

    Ok(())
}
