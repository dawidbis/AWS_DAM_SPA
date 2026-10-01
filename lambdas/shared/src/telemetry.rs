//! Konfiguracja logowania strukturalnego (JSON do CloudWatch).

use tracing_subscriber::EnvFilter;

/// Inicjalizuje `tracing` z wyjściem JSON.
///
/// Poziom logów sterowany zmienną `RUST_LOG` (domyślnie `info`). Bez
/// znaczników czasu i kolorów, bo CloudWatch dodaje własny czas zdarzenia.
pub fn init() {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_current_span(false)
        .with_ansi(false)
        .without_time()
        .with_target(false)
        .init();
}
