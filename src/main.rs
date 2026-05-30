mod db;

use clap::Parser;
use db::{DbConfig, check_target_schema, create_target_schema};
use postgres::Client;
use std::time::Instant;

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

    let target_db_config: DbConfig = DbConfig {
        host: args.target_host,
        username: args.target_username,
        password: args.target_password,
        port: args.target_port,
        database: args.target_database,
        require_tls: args.target_requires_tls,
        schema: args.target_schema,
    };

    let mut source_client = match source_db_config.create_client() {
        Ok(client) => client,
        Err(e) => {
            println!("ERROR: {:#?}", e);
            return;
        }
    };

    let mut target_client: Client = match target_db_config.create_client() {
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
    let clear_target_schema = match check_target_schema(&mut target_transaction, &target_db_config)
    {
        Ok(x) => x,
        Err(e) => {
            println!("ERROR: {:#?}", e);
            return;
        }
    };
    if !clear_target_schema {
        return; // end program
    }

    let start_time = Instant::now();

    // this is the single badass funciton that will do all the stuff we need
    // to "clone" a source database to a target database
    let final_result = match create_target_schema(
        &mut source_client,
        source_db_config,
        &mut target_transaction,
    ) {
        Ok(x) => x,
        Err(e) => {
            println!("{:?}", e);
            return;
        }
    };

    match target_transaction.commit() {
        Ok(_) => {}
        Err(e) => {
            println!("ERROR: {:#?}", e);
            return;
        }
    }

    println!(
        "
========================================
 Database schema cloned successfully
========================================

Tables created: {}
Columns created: {}
Views created: {}
Completed in: {:.2?}

",
        final_result.tables_created,
        final_result.columns_created,
        final_result.views_created,
        start_time.elapsed()
    );
}
