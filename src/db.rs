use native_tls::TlsConnector;
use postgres::{Client, Error as PostgresError, NoTls};
use postgres_native_tls::MakeTlsConnector;
use std::error::Error;

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

struct Column {
    pub name: String,
    pub data_type: ColumnDataType,
    pub is_nullable: bool,
}

struct ForeignKey {
    pub name: String,
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
        c.column_default
    FROM information_schema.columns c
    WHERE c.table_schema = $1
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
        let column_data_type = return_column_data_type(udt_name)?;

        for t in tables.iter_mut() {
            if t.name == table_name {
                t.columns.push(Column {
                    name: col_name.trim().to_string(),
                    data_type: column_data_type,
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
            col_queries.push(format!("{} {}", c.name, c.data_type.to_sql()))
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
            Err(_) => {
                println!(
                    "\nYou must install the {} extension on the target server before running.",
                    ext
                )
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
            REFERENCES users({});
        ",
                    table.name, fk.name, fk.references_column, fk.references_table
                ),
                &[],
            )?;
        }
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
