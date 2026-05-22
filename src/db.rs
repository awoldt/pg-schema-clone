use native_tls::TlsConnector;
use postgres::{Client, Error as PostgresError, NoTls, Transaction};
use postgres_native_tls::MakeTlsConnector;
use std::{collections::HashMap, error::Error};

// this contains all the info we need about the source db
// that needs to be applied to the target db
pub struct DbStructureResult {
    tables: Vec<Table>,
    extensions: Vec<DbExtension>,
}

pub struct DbExtension {
    name: String,
}

pub struct Table {
    pub name: String,
    columns: Vec<Column>,
}

struct Column {
    pub name: String,
    pub data_type: ColumnDataType,
    pub is_nullable: bool,
    pub is_primary_key: bool,
    pub foreign_key_details: Option<ForeignKey>,
}

struct ForeignKey {
    pub name: String,
    pub references_table: String,  // the table the fk points to
    pub references_column: String, // the column the fk points to (part of the table it points to)
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

pub enum ColumnDataType {
    SmallInt,
    Integer,
    BigInteger,
    Decimal,

    Text,
    CharacterVarying,
    Character,

    Boolean,

    Date,
    Time,
    TimeWithTZ,
    Timestamp,
    Interval,

    Json,
    JsonB,
    UUID,

    Bytea,

    Array(Box<ColumnDataType>),

    Custom(String), // IMPORTANT - this is basically any column type that is not part of default postgres
}

impl ColumnDataType {
    pub fn to_sql(&self) -> String {
        // this function will take the enum vairiant and return the valid postgres sql string (udt string)
        match self {
            ColumnDataType::SmallInt => "SMALLINT".to_string(),
            ColumnDataType::Integer => "INTEGER".to_string(),
            ColumnDataType::BigInteger => "BIGINT".to_string(),
            ColumnDataType::Decimal => "NUMERIC".to_string(),
            ColumnDataType::Text => "TEXT".to_string(),
            ColumnDataType::CharacterVarying => "VARCHAR".to_string(),
            ColumnDataType::Character => "CHAR".to_string(),
            ColumnDataType::Boolean => "BOOLEAN".to_string(),
            ColumnDataType::Date => "DATE".to_string(),
            ColumnDataType::Time => "TIME".to_string(),
            ColumnDataType::TimeWithTZ => "TIME WITH TIME ZONE".to_string(),
            ColumnDataType::Timestamp => "TIMESTAMP".to_string(),
            ColumnDataType::Interval => "INTERVAL".to_string(),
            ColumnDataType::Json => "JSON".to_string(),
            ColumnDataType::JsonB => "JSONB".to_string(),
            ColumnDataType::UUID => "UUID".to_string(),
            ColumnDataType::Bytea => "BYTEA".to_string(),
            ColumnDataType::Array(inner_type) => {
                format!("{}[]", inner_type.to_sql())
            }
            ColumnDataType::Custom(raw_type) => raw_type.to_string(),
        }
    }
}

pub fn get_db_structure(
    client: &mut Client,
    schema: &str,
) -> Result<DbStructureResult, Box<dyn Error>> {
    let table_structure_results = client.query(
        "SELECT
            c.table_name,
            c.column_name,

            CASE
                WHEN c.is_nullable = 'YES' THEN TRUE
                ELSE FALSE
            END AS is_nullable,

            c.udt_name,
            c.character_maximum_length,
            c.column_default,

            CASE
                WHEN pk_tc.constraint_type = 'PRIMARY KEY' THEN TRUE
                ELSE FALSE
            END AS is_primary_key,

            CASE
                WHEN fk_tc.constraint_type = 'FOREIGN KEY' THEN TRUE
                ELSE FALSE
            END AS is_foreign_key,

            fk_tc.constraint_name AS foreign_key_name,
            fk_ccu.table_name AS foreign_table_name,
            fk_ccu.column_name AS foreign_column_name

        FROM information_schema.columns c

        LEFT JOIN information_schema.key_column_usage pk_kcu
            ON c.table_name = pk_kcu.table_name
            AND c.column_name = pk_kcu.column_name
            AND c.table_schema = pk_kcu.table_schema

        LEFT JOIN information_schema.table_constraints pk_tc
            ON pk_kcu.constraint_name = pk_tc.constraint_name
            AND pk_kcu.table_schema = pk_tc.table_schema
            AND pk_tc.constraint_type = 'PRIMARY KEY'

        LEFT JOIN information_schema.key_column_usage fk_kcu
            ON c.table_name = fk_kcu.table_name
            AND c.column_name = fk_kcu.column_name
            AND c.table_schema = fk_kcu.table_schema

        LEFT JOIN information_schema.table_constraints fk_tc
            ON fk_kcu.constraint_name = fk_tc.constraint_name
            AND fk_kcu.table_schema = fk_tc.table_schema
            AND fk_tc.constraint_type = 'FOREIGN KEY'

        LEFT JOIN information_schema.constraint_column_usage fk_ccu
            ON fk_tc.constraint_name = fk_ccu.constraint_name
            AND fk_tc.table_schema = fk_ccu.table_schema

        WHERE c.table_schema = $1

        ORDER BY c.table_name, c.ordinal_position;",
        &[&schema],
    )?;

    let mut tables_and_columns: HashMap<String, Vec<Column>> =
        HashMap::<String, Vec<Column>>::new(); // Vec<table, all columns>
    for row in table_structure_results {
        let table_name: &str = row.get("table_name");
        let col_name: &str = row.get("column_name");
        let is_nullable: bool = row.get("is_nullable");
        let udt_name: &str = row.get("udt_name"); // udt stands for "user defined type"... the internal name postgres uses for a column type
        let character_max_length: Option<i32> = row.get("character_maximum_length");
        let column_default: Option<&str> = row.get("column_default");
        let is_primary_key: bool = row.get("is_primary_key");
        let is_foreign_key: bool = row.get("is_foreign_key");

        // determine if this column is a foreign key that points to another table
        let mut foreign_key_details: Option<ForeignKey> = None;
        if is_foreign_key {
            let name: Option<&str> = row.get("foreign_key_name");
            let table: Option<&str> = row.get("foreign_table_name");
            let column: Option<&str> = row.get("foreign_column_name");

            foreign_key_details = match (name, table, column) {
                (Some(name), Some(table), Some(column)) => Some(ForeignKey {
                    name: name.to_string(),
                    references_table: table.to_string(),
                    references_column: column.to_string(),
                }),
                _ => None,
            };
        }

        let column_data_type = return_column_data_type(udt_name)?;

        // now construct the hashmap of tables
        tables_and_columns
            .entry(String::from(table_name))
            .or_insert(vec![])
            .push(Column {
                name: String::from(col_name),
                data_type: column_data_type,
                is_nullable,
                is_primary_key,
                foreign_key_details,
            });
    }

    let mut tables: Vec<Table> = vec![];
    for (k, v) in tables_and_columns {
        tables.push(Table {
            name: k,
            columns: v,
        });
    }

    // we need to grab all the extentions a database has also
    // to apply to the target database
    let extension_reults = client.query(
        "
    SELECT
    extname AS extension_name,
    extnamespace::regnamespace::text AS extension_schema
    FROM pg_extension
    WHERE extnamespace::regnamespace::text = $1
    ORDER BY extname;
",
        &[&schema],
    )?;

    let mut db_extensions: Vec<DbExtension> = vec![];
    for extension in extension_reults {
        let extension_name: &str = extension.get("extension_name");
        db_extensions.push(DbExtension {
            name: extension_name.to_string(),
        });
    }

    Ok(DbStructureResult {
        tables: tables,
        extensions: db_extensions,
    })
}

pub fn generate_create_table_query(table: &Table) -> String {
    let mut cols: Vec<String> = vec![];
    // create each columns sql definitions

    for col in &table.columns {
        let mut col_str = format!("{} {} ", col.name.trim(), col.data_type.to_sql().trim()); // "is_verified BOOLEAN"
        if !col.is_nullable {
            col_str.push_str("NOT NULL ")
        } // "is_verified BOOLEAN NOT NULL"
        if col.is_primary_key {
            col_str.push_str("PRIMARY KEY")
        } // "is_verified BOOLEAN NOT NULL PRIMARY KEY"

        cols.push(col_str);
    }
    let col_sql_defs = cols.join(",");

    format!("CREATE TABLE {} ({});", table.name, col_sql_defs)
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
    let db_structure = get_db_structure(source_client, &source_db_config.schema)?;

    // add the extensions to the db first before adding all the
    // tables and columns
    for ext in &db_structure.extensions {
        match target_client.execute(
            &format!("CREATE EXTENSION IF NOT EXISTS {};", ext.name),
            &[],
        ) {
            Ok(_) => {}
            Err(_) => {
                println!(
                    "\nYou must install the {} extension on the target server before running.",
                    ext.name
                )
            }
        }
    }

    // generate the CREATE query for each table
    // and exectute against the target database!
    for t in &db_structure.tables {
        target_client.execute(&generate_create_table_query(&t), &[])?;
    }

    // once all the tables are created and ready, we need to add foreign keys
    let mut fk_queries: Vec<String> = vec![];
    for table in &db_structure.tables {
        for col in &table.columns {
            if let Some(x) = &col.foreign_key_details {
                fk_queries.push(format!(
                    "
                    ALTER TABLE {}
                    ADD CONSTRAINT {}
                    FOREIGN KEY ({})
                    REFERENCES {};
                ",
                    table.name, x.name, x.references_column, x.references_table
                ))
            }
        }
    }

    for q in &fk_queries {
        target_client.execute(q, &[])?;
    }

    Ok(())
}

pub fn return_column_data_type(raw_type: &str) -> Result<ColumnDataType, String> {
    // this function will take in the raw "udt" string for postgres and translate it
    // to a valid ColumnDataType

    // arrays need to be handled special
    if raw_type.starts_with("_") {
        return match raw_type {
            "_int2" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::SmallInt))),
            "_int4" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::Integer))),
            "_int8" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::BigInteger))),

            "_numeric" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::Decimal))),

            "_text" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::Text))),

            "_varchar" => Ok(ColumnDataType::Array(Box::new(
                ColumnDataType::CharacterVarying,
            ))),
            "_bpchar" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::Character))),

            "_bool" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::Boolean))),

            "_date" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::Date))),

            "_time" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::Time))),
            "_timetz" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::TimeWithTZ))),

            "_timestamp" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::Timestamp))),

            "_interval" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::Interval))),

            "_json" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::Json))),
            "_jsonb" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::JsonB))),

            "_uuid" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::UUID))),

            "_bytea" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::Bytea))),
            _ => Err(format!("unknown postgres array type: {}", raw_type)),
        };
    }

    match raw_type {
        "int2" => Ok(ColumnDataType::SmallInt),
        "int4" => Ok(ColumnDataType::Integer),
        "int8" => Ok(ColumnDataType::BigInteger),

        "numeric" => Ok(ColumnDataType::Decimal),

        "text" => Ok(ColumnDataType::Text),

        "varchar" => Ok(ColumnDataType::CharacterVarying),
        "bpchar" => Ok(ColumnDataType::Character),

        "bool" => Ok(ColumnDataType::Boolean),

        "date" => Ok(ColumnDataType::Date),

        "time" => Ok(ColumnDataType::Time),
        "timetz" => Ok(ColumnDataType::TimeWithTZ),
        "timestamptz" => Ok(ColumnDataType::TimeWithTZ),

        "timestamp" => Ok(ColumnDataType::Timestamp),

        "interval" => Ok(ColumnDataType::Interval),

        "json" => Ok(ColumnDataType::Json),
        "jsonb" => Ok(ColumnDataType::JsonB),

        "uuid" => Ok(ColumnDataType::UUID),

        "bytea" => Ok(ColumnDataType::Bytea),

        _ => Ok(ColumnDataType::Custom((raw_type.to_string()))), // DEFAULT FALLS BACK TO CUSTOM COLUMN TYPE
    }
}
