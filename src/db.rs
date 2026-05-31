use native_tls::TlsConnector;
use postgres::{Client, Error as PostgresError, NoTls};
use postgres_native_tls::MakeTlsConnector;
use std::io::{self, Read, Write};
use std::{collections::HashMap, error::Error};

// this contains all the info we need about the source db
// that needs to be applied to the target db
pub struct DbStructureResult {
    pub tables: Vec<Table>,
    extensions: Vec<String>,
    views: Vec<View>,
}

pub struct Table {
    name: String,
    columns: Vec<Column>,
    primary_keys: Vec<PrimaryKey>,
    foreign_keys: Vec<ForeignKey>,
}

struct View {
    name: String,
    definition: String,
}

struct ColumnType {
    udt_name: String,         // raw postgres internal name type for column
    postgres_type_kind: char, // what 'type' of column (base 'b', enum 'e', domain 'd', etc...)
    enum_values: Option<Vec<String>>,
    varhcar_max_characters: Option<i32>,
}

struct Column {
    name: String,
    data_type: ColumnType,
    is_nullable: bool,
    default_value: Option<String>,
}

#[derive(Debug)]
struct ForeignKey {
    constraint_name: String,
    columns: Vec<String>,
    references_table: String,        // the table the fk points to
    references_columns: Vec<String>, // the columns the fk points to (these columns belong to the "references_table")
}

#[derive(Debug)]
struct PrimaryKey {
    constraint_name: String,
    columns: Vec<String>,
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

pub struct CreatTargetSchemaResult {
    pub tables_created: i32,
    pub columns_created: i32,
    pub views_created: i32,
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

pub fn get_db_structure(
    client: &mut Client,
    schema: &str,
) -> Result<DbStructureResult, Box<dyn Error>> {
    let mut tables: Vec<Table> = vec![];
    let mut extensions: Vec<String> = vec![];
    let mut views: Vec<View> = vec![];

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
        c.column_default,

        t.typtype AS postgres_type_kind,

        array_agg(e.enumlabel ORDER BY e.enumsortorder)
            FILTER (WHERE e.enumlabel IS NOT NULL) AS enum_values

    FROM information_schema.columns c

    LEFT JOIN pg_type t
        ON t.typname = c.udt_name

    LEFT JOIN pg_enum e
        ON e.enumtypid = t.oid

    WHERE c.table_schema = $1

    GROUP BY
        c.table_name,
        c.column_name,
        c.is_nullable,
        c.udt_name,
        c.character_maximum_length,
        c.column_default,
        c.ordinal_position,
        t.typtype

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
        let postgres_type_kind: i8 = c.get("postgres_type_kind");
        let enum_values: Option<Vec<String>> = c.get("enum_values");

        for t in tables.iter_mut() {
            if t.name == table_name {
                t.columns.push(Column {
                    name: col_name.trim().to_string(),
                    data_type: ColumnType {
                        udt_name: udt_name.trim().to_string(),
                        postgres_type_kind: postgres_type_kind as u8 as char,
                        enum_values: enum_values,
                        varhcar_max_characters: character_max_length,
                    },
                    is_nullable: is_nullable,
                    default_value: column_default.map(|x| x.to_string()),
                });
                break;
            }
        }
    }

    // get all the views
    let views_results = client.query(
        "
        SELECT
            viewname,
            definition
        FROM pg_views
        WHERE schemaname = $1
        ORDER BY viewname;
    ",
        &[&schema],
    )?;

    for v in &views_results {
        let name: &str = v.get("viewname");
        let definition: &str = v.get("definition");

        views.push(View {
            name: name.to_string(),
            definition: definition.to_string(),
        });
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

    let mut pk_details: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new(); // <table, <contraint_name, vec<columns>>>
    for pk in &primary_key_results {
        let table_name: &str = pk.get("table_name");
        let pk_name: &str = pk.get("primary_key_name");
        let column_name: &str = pk.get("column_name");

        pk_details
            .entry(table_name.to_string())
            .or_insert(HashMap::new())
            .entry(pk_name.to_string())
            .or_insert(vec![])
            .push(column_name.to_string());
    }
    for t in tables.iter_mut() {
        if let Some(pk_details) = pk_details.get_mut(&t.name) {
            for (pk_contraint, columns) in pk_details {
                t.primary_keys.push(PrimaryKey {
                    constraint_name: pk_contraint.to_string(),
                    columns: columns.clone(),
                });
            }
        }
    }

    // get the foreign keys
    let foreign_key_results = client.query(
        "
       SELECT
        kcu.constraint_name AS foreign_key_name,
        kcu.table_name AS table_name,
        kcu.column_name AS column_name,
        ccu.table_name AS references_table,
        ccu.column_name AS references_column
    FROM information_schema.table_constraints tc

    JOIN information_schema.key_column_usage kcu
        ON tc.constraint_name = kcu.constraint_name
        AND tc.table_schema = kcu.table_schema

    JOIN information_schema.constraint_column_usage ccu
        ON tc.constraint_name = ccu.constraint_name
        AND tc.table_schema = ccu.table_schema

    WHERE tc.constraint_type = 'FOREIGN KEY'
    AND tc.table_schema = $1

    ORDER BY
        kcu.table_name,
        kcu.constraint_name,
        kcu.ordinal_position;
    ",
        &[&schema],
    )?;

    let mut fk_details: HashMap<String, HashMap<String, ForeignKey>> = HashMap::new(); // <table, <contraint_name, vec<columns>>>
    for fk in &foreign_key_results {
        let table_name: &str = fk.get("table_name");
        let fk_contraint_name: &str = fk.get("foreign_key_name");
        let column_name: &str = fk.get("column_name");
        let references_table: &str = fk.get("references_table");
        let references_column: &str = fk.get("references_column");

        fk_details
            .entry(table_name.to_string())
            .or_default()
            .entry(fk_contraint_name.to_string())
            .and_modify(|existing_fk| {
                existing_fk.columns.push(column_name.to_string());
                existing_fk
                    .references_columns
                    .push(references_column.to_string());
            })
            .or_insert(ForeignKey {
                constraint_name: fk_contraint_name.to_string(),
                columns: vec![column_name.to_string()],
                references_table: references_table.to_string(),
                references_columns: vec![references_column.to_string()],
            });
    }
    for t in tables.iter_mut() {
        if let Some(fk_details) = fk_details.get_mut(&t.name) {
            for (fk_constraint, fks) in fk_details {
                t.foreign_keys.push(ForeignKey {
                    constraint_name: fk_constraint.to_string(),
                    columns: fks.columns.clone(),
                    references_table: fks.references_table.clone(),
                    references_columns: fks.references_columns.clone(),
                });
            }
        }
    }

    Ok(DbStructureResult {
        tables,
        extensions,
        views,
    })
}

fn generate_create_table_query(tables: &Vec<Table>) -> String {
    let mut create_queries = vec![];
    for t in tables {
        let mut col_queries: Vec<String> = vec![];

        for c in &t.columns {
            let mut query: String;

            // apply a varchar with set number of characters
            if c.data_type.udt_name == "varchar" && c.data_type.varhcar_max_characters.is_some() {
                query = format!(
                    "{} varchar({})",
                    c.name,
                    c.data_type.varhcar_max_characters.unwrap()
                );
            } else {
                query = format!("{} {}", c.name, c.data_type.udt_name);
            }

            // check to see if the column is nullable
            if !c.is_nullable {
                query.push_str(" NOT NULL");
            }

            // add a default for a column if it needs one
            if c.default_value.is_some() {
                let default_value = c.default_value.as_ref().unwrap();
                // handle nextval with more modern way
                if default_value.starts_with("nextval(") {
                    query.push_str(" GENERATED BY DEFAULT AS IDENTITY");
                } else {
                    query.push_str(&format!(" DEFAULT {}", c.default_value.as_ref().unwrap()));
                }
            }

            col_queries.push(query);
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

pub fn check_target_schema(
    target_client: &mut postgres::Transaction<'_>,
    target_db_config: &DbConfig,
) -> Result<bool, PostgresError> {
    // this is mainly used to check if the target db already has tables/views
    // user must confirm if they want to continue or not

    let tables_query = target_client.query(
        "
        SELECT tablename
        FROM pg_tables
        WHERE schemaname = $1;
    ",
        &[&target_db_config.schema],
    )?;

    let views_query = target_client.query(
        "
        SELECT
            viewname,
            definition
            FROM pg_views
        WHERE schemaname = $1;
    ",
        &[&target_db_config.schema],
    )?;

    let num_of_target_tables = tables_query.len() as i32;
    let num_of_target_views = views_query.len() as i32;

    if num_of_target_tables == 0 && num_of_target_views == 0 {
        return Ok(true); // just continue, nothing to remove
    }

    loop {
        print!(
            "Your target database already has existing tables/views. Would you like to continue (y/n): "
        );

        io::stdout().flush().unwrap();
        let mut confirm = String::new();
        io::stdin()
            .read_line(&mut confirm)
            .expect("error while reading input");
        confirm = String::from(confirm.trim().to_lowercase());

        match confirm.as_str() {
            "n" => {
                return Ok(false); // end program
            }
            "y" => {
                remove_target_tables(target_client, &target_db_config.schema)?;

                return Ok(true);
            }
            _ => continue,
        }
    }
}

// this function will apply all the necessary extenstions, tables, views, and columns
// from the source database to the target database
pub fn create_target_schema(
    target_client: &mut postgres::Transaction<'_>,
    db_structure: &DbStructureResult,
) -> Result<CreatTargetSchemaResult, Box<dyn Error>> {
    // add the extensions to the db first before adding all the
    // tables and columns
    for ext in &db_structure.extensions {
        match target_client.execute(&format!("CREATE EXTENSION IF NOT EXISTS {};", ext), &[]) {
            Ok(_) => {}
            Err(e) => {
                println!(
                    "\nYou must install the {} extension on the target server before running.\n{:?}",
                    ext, e
                );
                return Err(Box::new(e));
            }
        }
    }

    // we need to check columns for any enum types and create those
    // before we attempt to create each table
    for t in &db_structure.tables {
        for c in &t.columns {
            if c.data_type.postgres_type_kind == 'e' {
                if let Some(enum_values) = &c.data_type.enum_values {
                    let mut quoted_enum_values: Vec<String> = vec![]; // need to add quotes around each enum value
                    for e in enum_values {
                        quoted_enum_values.push(format!("'{}'", e));
                    }

                    match target_client.execute(
                        &format!(
                            "CREATE TYPE {} as ENUM ({})",
                            c.data_type.udt_name,
                            quoted_enum_values.join(", ")
                        ),
                        &[],
                    ) {
                        Ok(_) => {}
                        Err(e) => {
                            println!(
                                "There was an error while creating the enum type '{}'\n{:?}",
                                c.data_type.udt_name, e
                            );
                            return Err(Box::new(e));
                        }
                    }
                }
            }
        }
    }

    // generate the CREATE query for each table
    // and exectute against the target database!
    target_client.batch_execute(&generate_create_table_query(&db_structure.tables))?;

    // once all the tables are created and ready, we need to add primary and foreign keys
    // ADD ALL PRIMARY KEYS FIRST
    for table in &db_structure.tables {
        let primary_keys: &Vec<PrimaryKey> = &table.primary_keys;

        for pk in primary_keys {
            target_client.execute(
                &format!(
                    "
            ALTER TABLE {}
            ADD CONSTRAINT {}
            PRIMARY KEY ({});
        ",
                    table.name,
                    pk.constraint_name,
                    pk.columns.join(", ")
                ),
                &[],
            )?;
        }
    }

    // now add all the foreign keys
    for table in &db_structure.tables {
        let foreign_keys: &Vec<ForeignKey> = &table.foreign_keys;

        // add the foreign keys
        for fk in foreign_keys {
            target_client.execute(
                &format!(
                    "
            ALTER TABLE {}
            ADD CONSTRAINT {}
            FOREIGN KEY ({})
            REFERENCES {}({});
        ",
                    table.name,
                    fk.constraint_name,
                    fk.columns.join(","),
                    fk.references_table,
                    fk.references_columns.join(",")
                ),
                &[],
            )?;
        }
    }

    // now add all the views
    for v in &db_structure.views {
        target_client.execute(
            &format!(
                "
            CREATE VIEW {} AS
            {};
        ",
                v.name, v.definition
            ),
            &[],
        )?;
    }

    let mut num_of_columns = 0;
    for t in &db_structure.tables {
        for _c in &t.columns {
            num_of_columns += 1;
        }
    }

    Ok(CreatTargetSchemaResult {
        tables_created: db_structure.tables.len() as i32,
        columns_created: num_of_columns,
        views_created: db_structure.views.len() as i32,
    })
}

pub fn copy_data(
    source_client: &mut Client,
    target_client: &mut postgres::Transaction<'_>,
    tables: &Vec<Table>,
) -> Result<(), Box<dyn Error>> {
    // copies data from source -> target database
    // we need to use 'copy' postgres query cause this might deal with
    // massive amounts of data

    for t in tables {
        let mut reader = source_client.copy_out(&format!("COPY {} TO STDOUT;", t.name))?;
        let mut buf = vec![];
        reader.read_to_end(&mut buf)?;

        let mut writer = target_client.copy_in(&format!("COPY {} FROM STDIN", t.name))?;
        writer.write_all(&mut buf)?;
        writer.finish()?;
    }

    Ok(())
}
