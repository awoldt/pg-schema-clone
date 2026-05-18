use crate::models::column::{Column, ColumnDataType};

use postgres::{Client, Error as PostgresError};
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
    pub fn build_postgres_conn_string(&self) -> String {
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
                WHEN tc.constraint_type = 'PRIMARY KEY' THEN TRUE
                ELSE FALSE
            END AS is_primary_key

        FROM information_schema.columns c

        LEFT JOIN information_schema.key_column_usage kcu
            ON c.table_name = kcu.table_name
            AND c.column_name = kcu.column_name
            AND c.table_schema = kcu.table_schema

        LEFT JOIN information_schema.table_constraints tc
            ON kcu.constraint_name = tc.constraint_name
            AND kcu.table_schema = tc.table_schema

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

pub fn generate_create_table_query(table: Table) -> String {
    let mut cols: Vec<String> = vec![];
    // create each columns sql definitions
    for col in table.columns {
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

pub fn target_db_has_tables(
    client: &mut postgres::Transaction<'_>,
    schema: &str,
) -> Result<i32, PostgresError> {
    // this will check to see if the target db has tables already in the specified schema

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
