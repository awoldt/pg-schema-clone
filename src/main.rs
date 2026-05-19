mod db;
mod models;

use clap::Parser;
use db::{
    DbConfig, generate_create_table_query, get_tables_structure, has_tables, remove_target_tables, insert_foreign_keys
};
use postgres::Client;
use std::io::{self, Write};

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
    let num_of_target_tables = match has_tables(&mut target_transaction, &target_db_config.schema) {
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
    for t in &tables_result {
        create_table_queries.push(generate_create_table_query(&t));
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

    // once all the tables are created  generate the foreign keys
    // for each column that needs one..
    // do this AFTER the tables are created so that these tables actually exist
    // and theres no errors
    match insert_foreign_keys(tables_result, &mut target_transaction) {
        Ok(_) => {}
        Err(e) => {
            println!("ERROR: {:#?}", e);
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

    println!("\n\n\n\n\n\nDONE OK!");
}
