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
    require_tls: bool, // presence based!
}

struct DbConfig {
    host: String,
    username: String,
    password: String,
    port: i32,
    database: String,
    require_tls: bool
}

fn main() {
    let args: UserInput = UserInput::parse();

    let source_db_config: DbConfig = DbConfig {
        host: args.source_host,
        username: args.source_username,
        password: args.source_password,
        port: args.source_port,
        database: args.source_database,
        require_tls: args.require_tls
    };

    let tls_connector = TlsConnector::new().unwrap();
    let tls = MakeTlsConnector::new(tls_connector);

    let source_conn_string: String = build_postgres_conn_string(source_db_config);
    let source_client_result: Result<Client, Error>;
    if args.require_tls {
        source_client_result = Client::connect(&source_conn_string, tls);
    } else {
        source_client_result = Client::connect(&source_conn_string, NoTls);
    }

    let _source_client: Client = match source_client_result {
        Ok(client) => client,
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
