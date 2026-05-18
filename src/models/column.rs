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

