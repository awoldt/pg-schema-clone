use crate::models::column::{Column, ColumnDataType, ForeignKey};

use native_tls::TlsConnector;
use postgres::{Client, Error as PostgresError, NoTls, Transaction};
use postgres_native_tls::MakeTlsConnector;
use std::{collections::HashMap, error::Error};

pub struct Table {
    pub name: String,
    pub columns: Vec<Column>,
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

pub fn get_tables_structure(
    client: &mut Client,
    schema: &str,
) -> Result<Vec<Table>, Box<dyn Error>> {
    let results = client.query(
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

    let mut tables: HashMap<String, Vec<Column>> = HashMap::<String, Vec<Column>>::new(); // Vec<table, all columns>
    for row in results {
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
        tables
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

    let mut answer: Vec<Table> = vec![];
    for (k, v) in tables {
        answer.push(Table {
            name: k,
            columns: v,
        });
    }

    Ok(answer)
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

pub fn insert_foreign_keys(
    tables: Vec<Table>,
    client: &mut Transaction<'_>,
) -> Result<(), Box<dyn Error>> {
    let mut queries: Vec<String> = vec![];

    for table in tables {
        for col in table.columns {
            if let Some(x) = col.foreign_key_details {
                queries.push(format!(
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

    for q in queries {
        client.execute(&q, &[])?;
    }

    Ok(())
}

pub fn insert_tables(
    source_client: &mut Client,
    source_db_config: DbConfig,
    target_client: &mut postgres::Transaction<'_>,
) -> Result<Vec<Table>, Box<dyn Error>> {
    // first get the entire schema table structure from the source database
    let schema_result = get_tables_structure(source_client, &source_db_config.schema)?;

    // generate the CREATE query for each table
    // and exectute against the target database!
    for t in &schema_result {
        target_client.execute(&generate_create_table_query(&t), &[])?;
    }

    Ok(schema_result)
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

            "_inet" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::Inet))),
            "_cidr" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::Cidr))),
            "_macaddr" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::Macaddr))),

            "_tsvector" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::TsVector))),
            "_tsquery" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::TsQuery))),

            "_point" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::Point))),
            "_line" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::Line))),
            "_polygon" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::Polygon))),
            "_circle" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::Circle))),

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

        "inet" => Ok(ColumnDataType::Inet),
        "cidr" => Ok(ColumnDataType::Cidr),
        "macaddr" => Ok(ColumnDataType::Macaddr),

        "tsvector" => Ok(ColumnDataType::TsVector),
        "tsquery" => Ok(ColumnDataType::TsQuery),

        "point" => Ok(ColumnDataType::Point),
        "line" => Ok(ColumnDataType::Line),
        "polygon" => Ok(ColumnDataType::Polygon),
        "circle" => Ok(ColumnDataType::Circle),

        _ => Err(format!("invalid column type {}", raw_type)),
    }
}
