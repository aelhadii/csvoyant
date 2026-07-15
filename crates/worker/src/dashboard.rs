//! Auto-generated dashboard metadata for a ready job: summary stats, per-column aggregations
//! queried from ClickHouse, and suggested charts chosen by column type.

use serde::Serialize;
use serde_json::{Value, json};
use shared::INGEST_TIMESTAMP_COLUMN;
use shared::clickhouse::escape_ident;
use sqlx::PgPool;
use uuid::Uuid;

use crate::clickhouse::ChClient;
use crate::error::IngestError;

/// How many distinct values to show in a suggested bar chart.
const TOP_VALUES: usize = 10;

/// What a column holds — drives which aggregations we run and which chart we suggest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ColumnKind {
    Numeric,
    Temporal,
    Boolean,
    Categorical,
}

impl ColumnKind {
    /// The chart type that best fits this kind of column.
    fn chart(self) -> &'static str {
        match self {
            ColumnKind::Numeric => "histogram",
            ColumnKind::Temporal => "time_series",
            ColumnKind::Boolean | ColumnKind::Categorical => "bar",
        }
    }
}

/// Peel `Nullable(X)` / `LowCardinality(X)` wrappers to reach the underlying type.
fn unwrap_type(t: &str) -> &str {
    let t = t.trim();
    for wrapper in ["Nullable(", "LowCardinality("] {
        if let Some(rest) = t.strip_prefix(wrapper)
            && let Some(inner) = rest.strip_suffix(')')
        {
            return unwrap_type(inner);
        }
    }
    t
}

/// Classify a ClickHouse type into a [`ColumnKind`].
pub fn classify(ch_type: &str) -> ColumnKind {
    let inner = unwrap_type(ch_type).to_ascii_lowercase();
    if inner.starts_with("int")
        || inner.starts_with("uint")
        || inner.starts_with("float")
        || inner.starts_with("decimal")
    {
        ColumnKind::Numeric
    } else if inner.starts_with("date") {
        // Date, Date32, DateTime, DateTime64(…)
        ColumnKind::Temporal
    } else if inner == "bool" || inner == "boolean" {
        ColumnKind::Boolean
    } else {
        ColumnKind::Categorical
    }
}

/// Build a single query computing every column's aggregates in one round-trip. Aliases are
/// positional (`c0_min`, …) so odd column names can't break the alias syntax.
fn build_stats_query(table: &str, columns: &[(String, String)]) -> String {
    let mut exprs = Vec::new();
    for (i, (name, ty)) in columns.iter().enumerate() {
        let c = format!("`{}`", escape_ident(name));
        exprs.push(format!("countIf({c} IS NULL) AS c{i}_nulls"));
        exprs.push(format!("uniq({c}) AS c{i}_distinct"));
        match classify(ty) {
            ColumnKind::Numeric => {
                exprs.push(format!("min({c}) AS c{i}_min"));
                exprs.push(format!("max({c}) AS c{i}_max"));
                exprs.push(format!("avg({c}) AS c{i}_avg"));
            }
            ColumnKind::Temporal => {
                exprs.push(format!("toString(min({c})) AS c{i}_min"));
                exprs.push(format!("toString(max({c})) AS c{i}_max"));
            }
            ColumnKind::Boolean | ColumnKind::Categorical => {}
        }
    }
    format!(
        "SELECT {} FROM `{}` \
         SETTINGS output_format_json_quote_64bit_integers = 0 FORMAT JSONEachRow",
        exprs.join(", "),
        escape_ident(table),
    )
}

/// Pick up to three charts: a time series, a histogram, and a bar — whichever types exist.
fn suggest_charts(columns: &[(String, String)]) -> Vec<(String, ColumnKind)> {
    let mut picks = Vec::new();
    for want in [
        ColumnKind::Temporal,
        ColumnKind::Numeric,
        ColumnKind::Categorical,
    ] {
        if let Some((name, _)) = columns
            .iter()
            .find(|(n, t)| classify(t) == want && n != INGEST_TIMESTAMP_COLUMN)
        {
            picks.push((name.clone(), want));
        }
    }
    picks
}

/// The most common values of a column, for a bar chart.
async fn top_values(ch: &ChClient, table: &str, column: &str) -> Result<Vec<Value>, IngestError> {
    let sql = format!(
        "SELECT toString(`{c}`) AS value, count() AS count FROM `{t}` \
         GROUP BY value ORDER BY count DESC LIMIT {TOP_VALUES} \
         SETTINGS output_format_json_quote_64bit_integers = 0 FORMAT JSONEachRow",
        c = escape_ident(column),
        t = escape_ident(table),
    );
    Ok(parse_json_lines(&ch.run(&sql).await?))
}

fn parse_json_lines(body: &str) -> Vec<Value> {
    body.lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

/// Build the dashboard config for a freshly-ingested table.
pub async fn generate(
    ch: &ChClient,
    table: &str,
    columns: &[(String, String)],
    row_count: i64,
) -> Result<Value, IngestError> {
    // One round-trip for all per-column aggregates.
    let body = ch.run(&build_stats_query(table, columns)).await?;
    let stats: Value = serde_json::from_str(body.lines().next().unwrap_or("{}"))
        .map_err(|e| IngestError::Permanent(format!("could not read column statistics: {e}")))?;

    let column_meta: Vec<Value> = columns
        .iter()
        .enumerate()
        .map(|(i, (name, ty))| {
            let kind = classify(ty);
            let mut s = json!({
                "nulls": stats.get(format!("c{i}_nulls")).cloned().unwrap_or(Value::Null),
                "distinct": stats.get(format!("c{i}_distinct")).cloned().unwrap_or(Value::Null),
            });
            for key in ["min", "max", "avg"] {
                if let Some(v) = stats.get(format!("c{i}_{key}")) {
                    s[key] = v.clone();
                }
            }
            json!({ "name": name, "type": ty, "kind": kind, "stats": s })
        })
        .collect();

    let mut charts = Vec::new();
    for (name, kind) in suggest_charts(columns) {
        let mut chart = json!({
            "kind": kind.chart(),
            "column": name,
            "title": format!("{} of {}", pretty(kind.chart()), name),
        });
        // Bar charts need the actual category counts; the rest are drawn from /data.
        if kind.chart() == "bar" {
            chart["top_values"] = json!(top_values(ch, table, &name).await?);
        }
        charts.push(chart);
    }

    Ok(json!({
        "summary": { "row_count": row_count, "column_count": columns.len() },
        "columns": column_meta,
        "charts": charts,
    }))
}

fn pretty(chart: &str) -> String {
    match chart {
        "histogram" => "Distribution".to_string(),
        "time_series" => "Trend".to_string(),
        _ => "Breakdown".to_string(),
    }
}

/// Persist (or replace) the dashboard for a job.
pub async fn store(
    pg: &PgPool,
    job_id: Uuid,
    user_id: Uuid,
    config: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO dashboards (job_id, user_id, config) VALUES ($1, $2, $3) \
         ON CONFLICT (job_id) DO UPDATE SET config = EXCLUDED.config, created_at = now()",
    )
    .bind(job_id)
    .bind(user_id)
    .bind(config)
    .execute(pg)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_clickhouse_types_through_wrappers() {
        assert_eq!(classify("Int64"), ColumnKind::Numeric);
        assert_eq!(classify("Nullable(Float64)"), ColumnKind::Numeric);
        assert_eq!(classify("Decimal(10, 2)"), ColumnKind::Numeric);
        assert_eq!(classify("Date"), ColumnKind::Temporal);
        assert_eq!(classify("Nullable(DateTime64(3))"), ColumnKind::Temporal);
        assert_eq!(classify("Bool"), ColumnKind::Boolean);
        assert_eq!(classify("String"), ColumnKind::Categorical);
        assert_eq!(
            classify("LowCardinality(Nullable(String))"),
            ColumnKind::Categorical
        );
    }

    #[test]
    fn suggests_one_chart_per_type_present() {
        let cols = vec![
            ("when".to_string(), "DateTime".to_string()),
            ("amount".to_string(), "Float64".to_string()),
            ("country".to_string(), "String".to_string()),
        ];
        let picks = suggest_charts(&cols);
        assert_eq!(picks.len(), 3);
        assert_eq!(picks[0], ("when".into(), ColumnKind::Temporal));
        assert_eq!(picks[1], ("amount".into(), ColumnKind::Numeric));
        assert_eq!(picks[2], ("country".into(), ColumnKind::Categorical));
        assert_eq!(picks[0].1.chart(), "time_series");
        assert_eq!(picks[1].1.chart(), "histogram");
        assert_eq!(picks[2].1.chart(), "bar");
    }

    #[test]
    fn suggests_only_what_exists() {
        let cols = vec![("name".to_string(), "String".to_string())];
        let picks = suggest_charts(&cols);
        assert_eq!(picks.len(), 1);
        assert_eq!(picks[0].1, ColumnKind::Categorical);
    }

    #[test]
    fn stats_query_covers_each_column_by_position() {
        let cols = vec![
            ("id".to_string(), "Int64".to_string()),
            ("tag".to_string(), "String".to_string()),
        ];
        let sql = build_stats_query("t", &cols);
        // Numeric gets min/max/avg; categorical only nulls/distinct.
        assert!(sql.contains("min(`id`) AS c0_min"));
        assert!(sql.contains("avg(`id`) AS c0_avg"));
        assert!(sql.contains("uniq(`tag`) AS c1_distinct"));
        assert!(!sql.contains("c1_avg"));
        assert!(sql.contains("output_format_json_quote_64bit_integers = 0"));
    }
}
