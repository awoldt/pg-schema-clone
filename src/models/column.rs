pub struct Column {
    pub name: String,
    pub data_type: ColumnDataType,
    pub is_nullable: bool,
    pub is_primary_key: bool,
}

pub enum ColumnDataType {
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

impl ColumnDataType {
    pub fn to_sql(&self) -> String {
        // this function will take the enum vairiant and return the valid postgres sql string (udt string)
        match self {
            ColumnDataType::SmallInt => "SMALLINT".to_string(),
            ColumnDataType::Integer => "INTEGER".to_string(),
            ColumnDataType::BigInteger => "BIGINT".to_string(),
            ColumnDataType::Decimal => "NUMERIC".to_string(),
            ColumnDataType::Text => "TEXT".to_string(),
            ColumnDataType::CharacterVarying => "VARCHAR".to_string(),
            ColumnDataType::Character => "CHAR".to_string(),
            ColumnDataType::Boolean => "BOOLEAN".to_string(),
            ColumnDataType::Date => "DATE".to_string(),
            ColumnDataType::Time => "TIME".to_string(),
            ColumnDataType::TimeWithTZ => "TIME WITH TIME ZONE".to_string(),
            ColumnDataType::Timestamp => "TIMESTAMP".to_string(),
            ColumnDataType::Interval => "INTERVAL".to_string(),
            ColumnDataType::Json => "JSON".to_string(),
            ColumnDataType::JsonB => "JSONB".to_string(),
            ColumnDataType::UUID => "UUID".to_string(),
            ColumnDataType::Bytea => "BYTEA".to_string(),
            ColumnDataType::Inet => "INET".to_string(),
            ColumnDataType::Cidr => "CIDR".to_string(),
            ColumnDataType::Macaddr => "MACADDR".to_string(),
            ColumnDataType::TsVector => "TSVECTOR".to_string(),
            ColumnDataType::TsQuery => "TSQUERY".to_string(),
            ColumnDataType::Point => "POINT".to_string(),
            ColumnDataType::Line => "LINE".to_string(),
            ColumnDataType::Polygon => "POLYGON".to_string(),
            ColumnDataType::Circle => "CIRCLE".to_string(),

            ColumnDataType::Array(inner_type) => {
                format!("{}[]", inner_type.to_sql())
            }
        }
    }
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
