use anyhow::bail;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Select};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::error::Error;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SortOrder {
    Asc,
    Desc,
}

impl From<SortOrder> for sea_orm::Order {
    fn from(value: SortOrder) -> Self {
        match value {
            SortOrder::Asc => sea_orm::Order::Asc,
            SortOrder::Desc => sea_orm::Order::Desc,
        }
    }
}

pub trait ListableTrait: QuerySelect {
    fn apply_filter(self, params: &ListQueryParams) -> anyhow::Result<Self>;
    fn apply_params(self, params: &ListQueryParams) -> anyhow::Result<Self>;
}

impl<ET> ListableTrait for Select<ET>
where
    ET: EntityTrait,
    <ET::Column as FromStr>::Err: Error + Send + Sync,
{
    fn apply_filter(mut self, params: &ListQueryParams) -> anyhow::Result<Self> {
        if let Some(filter) = &params.filter {
            let filter: BTreeMap<String, Value> = serde_json::from_str(filter)?;

            for (col, val) in filter.into_iter() {
                let column = ET::Column::from_str(&col)?;
                let filters = match val {
                    Value::String(v) => vec![Value::String(v)],
                    Value::Array(v) => v,
                    _ => {
                        bail!("Invalid filter value type: {col}. Expected string or array of strings.")
                    }
                };

                let mut values: Vec<sea_orm::Value> = vec![];
                for filter in filters {
                    if let Ok(uuid) = Uuid::deserialize(filter.clone()) {
                        values.push(uuid.into());
                    } else if let Value::String(s) = filter {
                        values.push(s.into());
                    } else {
                        values.push(filter.into())
                    }
                }
                self = self.filter(column.is_in(values));
            }
        }
        Ok(self)
    }

    fn apply_params(mut self, params: &ListQueryParams) -> anyhow::Result<Self> {
        if let Some(order) = &params.sort {
            let order: (String, SortOrder) = serde_json::from_str(order)?;

            self = self.order_by(ET::Column::from_str(&order.0)?, order.1.into())
        }
        if let Some(range) = &params.range {
            let range: (u64, u64) = serde_json::from_str(range)?;

            self = self.offset(range.0).limit(range.1 - range.0);
        }
        self = self.apply_filter(params)?;
        Ok(self)
    }
}

#[derive(Deserialize, Clone, Default)]
pub struct ListQueryParams {
    pub sort: Option<String>,
    pub range: Option<String>,
    pub filter: Option<String>,
}

pub struct PaginatedResult<T> {
    pub items: Vec<T>,
    pub total_count: u64,
}
