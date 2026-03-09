//! entity-relationship diagrams for infrahub
//!
//! fetches a graphql schema from an infrahub instance (or reads one from disk)
//! and renders entity relationships as a graphviz dot diagram.
//!
//! ## quick start
//!
//! ```bash
//! infrahub-erd --url http://localhost:8000 --token your-token > schema.dot
//! dot -Tpng schema.dot -o schema.png
//! ```

use clap::Parser;
use std::process;

mod dot;
mod error;
mod parse;

/// entity-relationship diagrams for infrahub
#[derive(Parser)]
#[command(name = "infrahub-erd", version, about)]
struct Cli {
    /// infrahub instance url
    #[arg(short, long, env = "INFRAHUB_URL")]
    url: Option<String>,

    /// api token
    #[arg(short, long, env = "INFRAHUB_TOKEN")]
    token: Option<String>,

    /// branch name
    #[arg(short, long)]
    branch: Option<String>,

    /// read schema from file instead of fetching
    #[arg(short = 'f', long)]
    schema_file: Option<String>,

    /// output file (default: stdout)
    #[arg(short, long)]
    output: Option<String>,

    /// hide attributes from entity nodes
    #[arg(long)]
    no_attributes: bool,

    /// skip ssl certificate verification
    #[arg(long)]
    no_ssl_verify: bool,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli).await {
        eprintln!("error: {err}");
        process::exit(1);
    }
}

async fn run(cli: Cli) -> error::Result<()> {
    let sdl = if let Some(path) = &cli.schema_file {
        std::fs::read_to_string(path)?
    } else {
        let url = cli.url.as_deref().ok_or_else(|| {
            error::Error::Config("--url or INFRAHUB_URL required when --schema-file not set".into())
        })?;
        let token = cli.token.as_deref().ok_or_else(|| {
            error::Error::Config(
                "--token or INFRAHUB_TOKEN required when --schema-file not set".into(),
            )
        })?;

        let config =
            infrahub::ClientConfig::new(url, token).with_ssl_verification(!cli.no_ssl_verify);
        let client = infrahub::Client::new(config)?;
        client.fetch_schema(cli.branch.as_deref()).await?
    };

    let schema = parse::parse_graphql_schema(&sdl)?;
    let output = dot::render(&schema, !cli.no_attributes);

    if let Some(path) = &cli.output {
        std::fs::write(path, &output)?;
    } else {
        print!("{output}");
    }

    Ok(())
}
