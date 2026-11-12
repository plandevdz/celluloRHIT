use clap::{Parser, Subcommand};
use server::config::Config;
use server::{app, connect, seed, worker, AppState, MIGRATOR};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "celluloplast", version, about = "Celluloplast | Parcours salarié")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Démarre l'API et l'interface web (applique les migrations).
    Serve,
    /// Démarre le worker (notifications, PDF en attente).
    Worker,
    /// Applique les migrations SQL puis s'arrête.
    Migrate,
    /// Crée un administrateur et affiche son lien d'activation à usage unique.
    CreateAdmin {
        #[arg(long)]
        email: String,
        #[arg(long)]
        name: String,
    },
    /// Charge un jeu de données FICTIF de démonstration (base vide uniquement).
    SeedDemo,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,tower_http=info"));
    if std::env::var("LOG_FORMAT").as_deref() == Ok("json") {
        tracing_subscriber::fmt().json().with_env_filter(filter).init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }

    let cli = Cli::parse();
    let config = Config::from_env()?;
    let pool = connect(&config.database_url).await?;

    match cli.command {
        Command::Serve => {
            MIGRATOR.run(&pool).await?;
            let bind = config.bind_addr.clone();
            let state = AppState::new(pool, config);
            let listener = tokio::net::TcpListener::bind(&bind).await?;
            tracing::info!(%bind, version = server::VERSION, "API démarrée");
            axum::serve(listener, app(state))
                .with_graceful_shutdown(async {
                    let _ = tokio::signal::ctrl_c().await;
                })
                .await?;
        }
        Command::Worker => {
            MIGRATOR.run(&pool).await?;
            worker::run(AppState::new(pool, config)).await?;
        }
        Command::Migrate => {
            MIGRATOR.run(&pool).await?;
            println!("Migrations appliquées.");
        }
        Command::CreateAdmin { email, name } => {
            MIGRATOR.run(&pool).await?;
            let link = seed::create_admin(&pool, &config, &email, &name).await?;
            println!("Administrateur créé : {email}");
            println!("Lien d'activation (usage unique, 72 h) :\n{link}");
        }
        Command::SeedDemo => {
            MIGRATOR.run(&pool).await?;
            seed::seed_demo(&pool).await?;
        }
    }
    Ok(())
}
