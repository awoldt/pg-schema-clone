mod models;

use std::{
    collections::HashMap,
    error::Error,
    io::{self, Read, Write},
};

use models::column::{Column, return_column_data_type};

use clap::Parser;
use native_tls::TlsConnector;
use postgres::{Client, Error as PostgresError, NoTls};
use postgres_native_tls::MakeTlsConnector;

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

    #[arg(long, default_value = "public")]
    source_schema: String,

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

    #[arg(long, default_value = "public")]
    target_schema: String,
}

struct DbConfig {
    host: String,
    username: String,
    password: String,
    port: i32,
    database: String,
    require_tls: bool,
    schema: String,
}

struct Table {
    name: String,
    columns: Vec<Column>,
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
        schema: args.source_schema,
    };
    let source_conn_string: String = build_postgres_conn_string(&source_db_config);

    let target_db_config: DbConfig = DbConfig {
        host: args.target_host,
        username: args.target_username,
        password: args.target_password,
        port: args.target_port,
        database: args.target_database,
        require_tls: args.target_requires_tls,
        schema: args.target_schema,
    };
    let target_conn_string: String = build_postgres_conn_string(&target_db_config);

    let tls_connector = match TlsConnector::builder().build() {
        Ok(v) => v,
        Err(_e) => {
            println!("error while establishing a TLS connector");
            return;
        }
    };

    let source_client_result: Result<Client, PostgresError>;
    if args.source_requires_tls {
        let tls = MakeTlsConnector::new(tls_connector.clone());
        source_client_result = Client::connect(&source_conn_string, tls);
    } else {
        source_client_result = Client::connect(&source_conn_string, NoTls);
    }

    let target_client_result: Result<Client, PostgresError>;
    if args.target_requires_tls {
        let tls = MakeTlsConnector::new(tls_connector.clone());
        target_client_result = Client::connect(&target_conn_string, tls);
    } else {
        target_client_result = Client::connect(&target_conn_string, NoTls);
    }

    let mut source_client: Client = match source_client_result {
        Ok(client) => client,
        Err(e) => {
            println!("ERROR: {:#?}", e);
            return;
        }
    };

    let mut target_client: Client = match target_client_result {
        Ok(client) => client,
        Err(e) => {
            println!("ERROR: {:#?}", e);
            return;
        }
    };

    // USE A TRANSACTION FOR ANYTHING THAT DEALS WITH THE TARGET DB
    let mut target_transaction: postgres::Transaction<'_> = match target_client.transaction() {
        Ok(x) => x,
        Err(e) => {
            println!("ERROR: {:#?}", e);
            return;
        }
    };

    // first check to see if the target db already has tables
    // in the specified schema
    // if so, the user must confirm to continue (will delete all those tables)
    let num_of_target_tables =
        match target_db_has_tables(&mut target_transaction, &target_db_config.schema) {
            Ok(x) => x,
            Err(e) => {
                println!("ERROR: {:#?}", e);
                return;
            }
        };

    if num_of_target_tables > 0 {
        loop {
            print!(
                "Your target database already has {} tables. Would you like to continue (y/n): ",
                num_of_target_tables
            );

            io::stdout().flush().unwrap();
            let mut confirm = String::new();
            io::stdin()
                .read_line(&mut confirm)
                .expect("error while reading input");
            confirm = String::from(confirm.trim().to_lowercase());

            match confirm.as_str() {
                "n" => {
                    return; // end program
                }
                "y" => {
                    match remove_target_tables(&mut target_transaction, &target_db_config.schema) {
                        Ok(x) => {}
                        Err(e) => {
                            println!("ERROR: {:#?}", e);
                            return;
                        }
                    }
                    break;
                }
                _ => continue,
            }
        }
    }

    // get all the source tables that will be cloned
    let tables_result = match get_tables_structure(&mut source_client, &source_db_config.schema) {
        Ok(x) => x,
        Err(e) => {
            println!("ERROR: {:#?}", e);
            return;
        }
    };

    // generate the CREATE query for each table
    let mut create_table_queries: Vec<String> = vec![];
    for t in tables_result {
        create_table_queries.push(generate_create_table_query(t));
    }

    for q in create_table_queries {
        match target_transaction.execute(&q, &[]) {
            Ok(_) => {}
            Err(e) => {
                println!("ERROR: {:#?}", e);
                return;
            }
        }
    }

    match target_transaction.commit() {
        Ok(_) => {}
        Err(e) => {
            println!("ERROR: {:#?}", e);
            return;
        }
    }

    println!("\n\n\n\n\n\nDONE OK!");
}

fn target_db_has_tables(
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

fn remove_target_tables(
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

fn build_postgres_conn_string(config: &DbConfig) -> String {
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

fn get_tables_structure(client: &mut Client, schema: &str) -> Result<Vec<Table>, Box<dyn Error>> {
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

fn generate_create_table_query(table: Table) -> String {
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
