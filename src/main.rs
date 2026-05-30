mod db;

use clap::Parser;
use db::{DbConfig, check_target_schema, create_target_schema, get_db_structure};
use postgres::Client;
use std::io::{self, Write};
use std::time::Instant;

use crate::ProgramAction::{Clone, CloneImport};
use crate::db::copy_data;

#[derive(PartialEq)]
enum ProgramAction {
    Clone,
    CloneImport,
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

    // before we do anything, ask the user if they want to "clone" or "clone + import"
    // "clone" will just apply the schema to the target database with no data
    // "clone + import" will apply the schema to the target database AND copy all data to the target database
    let mut program_action: ProgramAction;
    loop {
        print!(
            "Select an operation:\n1) Clone schema\n2) Clone schema and import data\nChoice [1-2]:"
        );
        io::stdout().flush().unwrap();
        let mut action = String::new();
        io::stdin()
            .read_line(&mut action)
            .expect("error while reading input");
        let action: i32 = match String::from(action.trim().to_lowercase()).parse() {
            Ok(x) => x,
            Err(e) => {
                println!("ERROR: {:#?}", e);
                return;
            }
        };
        if action == 1 {
            program_action = Clone;
            break;
        } else if action == 2 {
            program_action = CloneImport;
            break;
        } else {
            continue;
        }
    }

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
    // if so, the user must confirm to delete before continuing
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

    // first get the entire schema structure from the source database
    // this will include all the important details needed for cloning a schema
    let db_structure: db::DbStructureResult =
        match get_db_structure(&mut source_client, &source_db_config.schema) {
            Ok(x) => x,
            Err(e) => {
                println!("{:?}", e);
                return;
            }
        };

    // once we have the strucutre of the source schema, we can apply to the target database
    let final_result = match create_target_schema(&mut target_transaction, &db_structure) {
        Ok(x) => x,
        Err(e) => {
            println!("{:?}", e);
            return;
        }
    };

    // if the user wants to clone and import
    // copy all the data to target tables
    if program_action == CloneImport {
        match copy_data(
            &mut source_client,
            &mut target_transaction,
            &db_structure.tables,
        ) {
            Ok(x) => x,
            Err(e) => {
                println!("{:?}", e);
                return;
            }
        };
    }

    match target_transaction.commit() {
        Ok(_) => {}
        Err(e) => {
            println!("ERROR: {:#?}", e);
            return;
        }
    }

    println!(
        "
=======================================================
{}
=======================================================

Tables created: {}
Columns created: {}
Views created: {}
Completed in: {:.2?}

",
        if program_action == Clone {
            "Database schema cloned successfully"
        } else {
            "Database schema cloned and data imported successfully"
        },
        final_result.tables_created,
        final_result.columns_created,
        final_result.views_created,
        start_time.elapsed()
    );
}
