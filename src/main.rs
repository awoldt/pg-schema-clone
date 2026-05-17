use std::{collections::HashMap, error::Error, os::raw};

use clap::Parser;
use native_tls::TlsConnector;
use postgres::{Client, Error as PostgresError, NoTls};
use postgres_native_tls::MakeTlsConnector;

enum ColumnDataType {
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
    Inet,
    Cidr,
    Macaddr,
    TsVector,
    TsQuery,
    Point,
    Line,
    Polygon,
    Circle,
    Array(Box<ColumnDataType>), // this can represent all array types
}

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct UserInput {
    // -- source db configs
    #[arg(long, required = true)]
    source_host: String,

    #[arg(long, required = true)]
    source_username: String,

    #[arg(long, required = true)]
    source_password: String,

    #[arg(long, default_value = "5432")]
    source_port: i32,

    #[arg(long, required = true)]
    source_database: String,

    #[arg(long)]
    source_requires_tls: bool,

    // -- target db configs
    #[arg(long, required = true)]
    target_host: String,

    #[arg(long, required = true)]
    target_username: String,

    #[arg(long, required = true)]
    target_password: String,

    #[arg(long, default_value = "5432")]
    target_port: i32,

    #[arg(long, required = true)]
    target_database: String,

    #[arg(long)]
    target_requires_tls: bool,
}

struct DbConfig {
    host: String,
    username: String,
    password: String,
    port: i32,
    database: String,
    require_tls: bool,
}

struct Column {
    name: String,
    data_type: ColumnDataType,
    is_nullable: bool
}

fn main() {
    let args: UserInput = UserInput::parse();

    let source_db_config: DbConfig = DbConfig {
        host: args.source_host,
        username: args.source_username,
        password: args.source_password,
        port: args.source_port,
        database: args.source_database,
        require_tls: args.source_requires_tls,
    };

    let tls_connector = match TlsConnector::new() {
        Ok(v) => v,
        Err(_e) => {
            println!("error while establishing a TLS connector");
            return;
        }
    };

    let tls = MakeTlsConnector::new(tls_connector);
    let source_conn_string: String = build_postgres_conn_string(source_db_config);
    let source_client_result: Result<Client, PostgresError>;
    if args.source_requires_tls {
        source_client_result = Client::connect(&source_conn_string, tls);
    } else {
        source_client_result = Client::connect(&source_conn_string, NoTls);
    }

    let mut source_client: Client = match source_client_result {
        Ok(client) => client,
        Err(e) => {
            println!("ERROR: {:#?}", e);
            return;
        }
    };

    let tables_result = match get_tables_structure(&mut source_client, "public") {
        Ok(x) => x,
        Err(e) => {
            println!("ERROR: {:#?}", e);
            return;
        }
    };
}

fn build_postgres_conn_string(config: DbConfig) -> String {
    if config.require_tls {
        format!(
            "postgres://{}:{}@{}:{}/{}?sslmode=require",
            config.username, config.password, config.host, config.port, config.database
        )
    } else {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            config.username, config.password, config.host, config.port, config.database
        )
    }
}

fn get_tables_structure(
    client: &mut Client,
    schema: &str,
) -> Result<HashMap<String, Vec<Column>>, Box<dyn Error>> {
    let results = client.query(
        "SELECT
            c.table_name,
            c.column_name,
            c.is_nullable,
            c.udt_name,
            c.character_maximum_length,
            c.column_default
        FROM information_schema.columns c
        WHERE c.table_schema = $1
        ORDER BY c.table_name, c.ordinal_position;",
        &[&schema],
    )?;

    let mut tables: HashMap<String, Vec<Column>> = HashMap::<String, Vec<Column>>::new(); // Vec<table, all columns>

    for row in results {
        let table_name: &str = row.get("table_name");
        let col_name: &str = row.get("column_name");
        let is_nullable: &str = row.get("is_nullable");
        let udt_name: &str = row.get("udt_name"); // udt stands for "user defined type"... the internal name postgres uses for a column type
        let character_max_length: Option<i32> = row.get("character_maximum_length");
        let column_default: Option<&str> = row.get("column_default");

        let column_data_type = return_column_data_type(udt_name)?;

        // now construct the hashmap of tables
        tables
            .entry(String::from(table_name))
            .or_insert(vec![])
            .push(Column {
                name: String::from(col_name),
                data_type: column_data_type,
                is_nullable: if is_nullable == "YES" {true} else {false}
            });
    }

    Ok(tables)
}

fn return_column_data_type(raw_type: &str) -> Result<ColumnDataType, String> {
    // detect an array type
    if raw_type.starts_with("_") {
        return match raw_type {
            "_int2" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::SmallInt))),
            "_int4" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::Integer))),
            "_int8" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::BigInteger))),
    
            "_numeric" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::Decimal))),
    
            "_text" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::Text))),
    
            "_varchar" => Ok(ColumnDataType::Array(Box::new(ColumnDataType::CharacterVarying))),
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

        _ => Err(format!("invalid column type {}", raw_type))
    }
}
