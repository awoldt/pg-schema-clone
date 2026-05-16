use std::collections::HashMap;

use clap::Parser;
use native_tls::TlsConnector;
use postgres::{Client, Error, NoTls};
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
    let source_client_result: Result<Client, Error>;
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


    let tables_result = get_table_structure(&mut source_client);
    let tables =  match tables_result {
        Ok(x) => x,
           Err(e) => {
            println!("ERROR: {:#?}", e);
            return;
        }
    };

    println!("{:?}", tables);


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

fn get_table_structure(client: &mut Client) -> Result<HashMap<String, Vec<String>>, Error> {
    let results = client.query(
        "SELECT
		t.table_name,
		c.column_name
		FROM information_schema.tables t
		LEFT JOIN information_schema.columns c
		ON c.table_schema = t.table_schema
		AND c.table_name   = t.table_name
		WHERE t.table_schema =  $1
		ORDER BY t.table_name, c.ordinal_position;",
        &[&"public"],
    )?;

    let mut answer: HashMap<String, Vec<String>> = HashMap::<String, Vec<String>>::new(); // Vec<table, []columns>

    for row in results {
        let table_name: &str = row.get(0);
        let col_name: &str = row.get(1);
        if answer.contains_key(table_name) {
            // append this column to the table key
            if let Some(table_cols) = answer.get_mut(table_name) {
                table_cols.push(String::from(col_name))
            } 
        } else {
            // add this table to the hash and set the first column in the vector
            answer.insert(String::from(table_name), vec![String::from(col_name)]);
        }
    }

    Ok(answer)
}
