use clap::{Parser, Subcommand};
use serde_json::Value;
use spotifree_rs::{SearchResults, Session};

#[derive(Parser)]
#[command(
    name = "spotifree",
    version,
    about = "Unofficial Spotify client. No API key, no premium account."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Search tracks, artists, albums and playlists
    Search {
        /// Search text
        query: String,

        /// Results per category (1-100)
        #[arg(short, long, default_value_t = 5)]
        limit: usize,

        /// Print the raw typed results as JSON
        #[arg(long)]
        json: bool,

        /// Accept-Language value, such as en, en-US or fr-CA
        #[arg(long, default_value = "en")]
        language: String,
    },

    /// Run any persisted GraphQL operation and print its data object
    Query {
        /// Operation name, such as searchDesktop or getTrack
        operation: String,

        /// JSON variables object
        #[arg(default_value = "{}")]
        variables: String,

        #[arg(long, default_value = "en")]
        language: String,
    },

    /// Print details about the anonymous web-player token
    Token {
        #[arg(long, default_value = "en")]
        language: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Command::Search {
            query,
            limit,
            json,
            language,
        } => {
            let session = Session::with_language(&language).await?;
            let results = session.search(&query, limit).await?;

            if json {
                println!("{}", serde_json::to_string_pretty(&results)?);
            } else {
                print_results(&results);
            }
        }

        Command::Query {
            operation,
            variables,
            language,
        } => {
            let variables: Value = serde_json::from_str(&variables)?;
            let session = Session::with_language(&language).await?;
            let data = session.query(&operation, variables).await?;
            println!("{}", serde_json::to_string_pretty(&data)?);
        }

        Command::Token { language } => {
            let session = Session::with_language(&language).await?;
            session.authorize().await?;

            let token = session
                .token()
                .await
                .ok_or("authorization succeeded without a token")?;

            println!("client id:    {}", token.client_id);
            println!("anonymous:    {}", token.is_anonymous);
            println!("expires in:   {}s", token.expires_in_ms() / 1000);
        }
    }

    Ok(())
}

fn print_results(results: &SearchResults) {
    println!("query: {}", results.query);

    if !results.tracks.is_empty() {
        println!("\ntracks");
        for track in &results.tracks {
            println!(
                "  {} — {}\n    {}",
                track.name,
                track.artists.join(", "),
                track.uri
            );
        }
    }

    if !results.artists.is_empty() {
        println!("\nartists");
        for artist in &results.artists {
            println!("  {}\n    {}", artist.name, artist.uri);
        }
    }

    if !results.albums.is_empty() {
        println!("\nalbums");
        for album in &results.albums {
            println!(
                "  {} — {}\n    {}",
                album.name,
                album.artists.join(", "),
                album.uri
            );
        }
    }

    if !results.playlists.is_empty() {
        println!("\nplaylists");
        for playlist in &results.playlists {
            let owner = playlist.owner.as_deref().unwrap_or("unknown");
            println!("  {} (by {})\n    {}", playlist.name, owner, playlist.uri);
        }
    }
}
