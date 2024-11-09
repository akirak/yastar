use clap::Parser;
use clap::Subcommand;
use clap::ValueEnum;
use tokio;
use tracing::info;
use yastar::render_star_history_by_language;
use yastar::render_total_star_history;
use yastar::update_database;

#[derive(Parser, Debug)]
#[command(name = "yastar")]
#[command(about = "Star history for your GitHub profile")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, ValueEnum, Copy, Clone, PartialEq, Eq)]
enum HistoryChartType {
    Language,
    Total,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Update the local database specified in the config.
    Update,

    // Render a chart to the given file
    Chart {
        #[arg(required = true)]
        path: String,
        #[arg(
            long = "type",
            help = "Set the history chart type",
            require_equals = true,
            num_args = 0..=1,
            default_value_t = HistoryChartType::Language,
            value_enum,
            value_name = "TYPE"
        )]
        chart_type: HistoryChartType,
    },

    /// Print the config.
    Config,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // continue even if .env does not exist
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Update => {
            let mut duckdb = duckdb_open_env()?;
            update_database(&mut duckdb).await?;
        }
        Commands::Config => {
            let conn_string = duckdb_connection()?;
            println!("Database (duckdb): {}", conn_string);
        }
        Commands::Chart { chart_type, path } => {
            let mut duckdb = duckdb_open_env()?;
            match chart_type {
                HistoryChartType::Language => {
                    render_star_history_by_language(&mut duckdb, path.as_str())?;
                }
                HistoryChartType::Total => {
                    render_total_star_history(&mut duckdb, path.as_str())?;
                }
            }
        }
    }

    Ok(())
}

fn duckdb_open_env() -> anyhow::Result<duckdb::Connection> {
    let conn_string = duckdb_connection()?;
    info!(path = conn_string, "opening database");
    let conn = duckdb::Connection::open(conn_string)?;
    Ok(conn)
}

fn duckdb_connection() -> anyhow::Result<String> {
    let conn_string = std::env::var("DUCKDB_DATABASE")?;
    Ok(conn_string)
}
