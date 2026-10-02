//! Demon ClamAV (`clamd`) uruchamiany wewnątrz środowiska Lambdy.
//!
//! Wczytanie bazy sygnatur trwa kilkadziesiąt sekund, więc demon startuje
//! przy pierwszym wywołaniu i zostaje w ciepłym środowisku na kolejne
//! skany. Komunikacja przez gniazdo uniksowe (protokół clamd, komendy `z`).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::process::{Child, Command};

/// Wynik skanu pliku.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Clean,
    Infected(String),
    /// Skan się nie udał lub był niepełny: plik NIE jest dopuszczany.
    Failed(String),
}

/// Interpretuje odpowiedź na `SCAN`: `<ścieżka>: OK`, `<ścieżka>: <sygnatura> FOUND`
/// albo `<ścieżka>: <błąd> ERROR`. Wszystko, czego nie rozpoznajemy, to porażka
/// (fail closed, rozdział 7.2).
#[must_use]
pub fn parse_scan_response(response: &str) -> Verdict {
    let response = response.trim_end_matches('\0').trim();
    let Some((_, result)) = response.rsplit_once(": ") else {
        return Verdict::Failed(format!("nieczytelna odpowiedź clamd: {response}"));
    };
    if result == "OK" {
        return Verdict::Clean;
    }
    if let Some(signature) = result.strip_suffix(" FOUND") {
        // Przekroczone limity skanowania (rozmiar, rekursja) to nie infekcja,
        // ale też nie dowód czystości: plik nie przechodzi.
        if signature.starts_with("Heuristics.Limits") {
            return Verdict::Failed(format!("skan niepełny: {signature}"));
        }
        return Verdict::Infected(signature.to_owned());
    }
    Verdict::Failed(format!("błąd clamd: {result}"))
}

pub struct Clamd {
    socket: PathBuf,
    config: PathBuf,
    child: Option<Child>,
    startup_timeout: Duration,
}

impl Clamd {
    #[must_use]
    pub fn new(socket: impl Into<PathBuf>, config: impl Into<PathBuf>, startup_timeout: Duration) -> Self {
        Self {
            socket: socket.into(),
            config: config.into(),
            child: None,
            startup_timeout,
        }
    }

    async fn command(&self, command: &str) -> Result<String, String> {
        let mut stream = UnixStream::connect(&self.socket)
            .await
            .map_err(|e| e.to_string())?;
        stream
            .write_all(format!("z{command}\0").as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .await
            .map_err(|e| e.to_string())?;
        Ok(response.trim_end_matches('\0').trim().to_owned())
    }

    async fn is_ready(&self) -> bool {
        matches!(self.command("PING").await.as_deref(), Ok("PONG"))
    }

    /// Uruchamia demona, jeśli nie działa, i czeka, aż wczyta bazę sygnatur.
    ///
    /// # Errors
    ///
    /// Gdy demon nie wystartuje w czasie `startup_timeout`.
    pub async fn ensure_started(&mut self) -> Result<(), String> {
        if self.is_ready().await {
            return Ok(());
        }
        let alive = self
            .child
            .as_mut()
            .is_some_and(|child| matches!(child.try_wait(), Ok(None)));
        if !alive {
            let _ = tokio::fs::remove_file(&self.socket).await;
            tracing::info!("starting clamd");
            let child = Command::new("clamd")
                .arg("--foreground")
                .arg(format!("--config-file={}", self.config.display()))
                .stdin(Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .map_err(|e| format!("nie można uruchomić clamd: {e}"))?;
            self.child = Some(child);
        }
        let deadline = tokio::time::Instant::now() + self.startup_timeout;
        while tokio::time::Instant::now() < deadline {
            if self.is_ready().await {
                tracing::info!("clamd ready");
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        Err("clamd nie wystartował w wyznaczonym czasie".to_owned())
    }

    /// Wersja silnika i bazy sygnatur, np. `ClamAV 1.0.7/27412/...`.
    pub async fn version(&self) -> String {
        self.command("VERSION")
            .await
            .unwrap_or_else(|_| "unknown".to_owned())
    }

    pub async fn scan(&self, path: &Path) -> Verdict {
        match self.command(&format!("SCAN {}", path.display())).await {
            Ok(response) => parse_scan_response(&response),
            Err(error) => Verdict::Failed(format!("brak połączenia z clamd: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_file() {
        assert_eq!(parse_scan_response("/tmp/a: OK\0"), Verdict::Clean);
    }

    #[test]
    fn eicar_is_infected() {
        assert_eq!(
            parse_scan_response("/tmp/a: Win.Test.EICAR_HDB-1 FOUND"),
            Verdict::Infected("Win.Test.EICAR_HDB-1".to_owned())
        );
    }

    #[test]
    fn exceeded_limits_fail_closed() {
        assert!(matches!(
            parse_scan_response("/tmp/a: Heuristics.Limits.Exceeded.MaxFileSize FOUND"),
            Verdict::Failed(_)
        ));
    }

    #[test]
    fn errors_and_garbage_fail_closed() {
        assert!(matches!(
            parse_scan_response("/tmp/a: lstat() failed: No such file. ERROR"),
            Verdict::Failed(_)
        ));
        assert!(matches!(parse_scan_response(""), Verdict::Failed(_)));
        assert!(matches!(
            parse_scan_response("COMMAND READ TIMED OUT"),
            Verdict::Failed(_)
        ));
    }

    #[tokio::test]
    async fn unreachable_daemon_fails_closed() {
        let clamd = Clamd::new(
            "/nonexistent/clamd.sock",
            "/nonexistent.conf",
            Duration::from_millis(10),
        );
        assert!(matches!(
            clamd.scan(Path::new("/tmp/x")).await,
            Verdict::Failed(_)
        ));
    }
}

#[cfg(test)]
mod integration {
    //! Test z prawdziwym clamd: `CLAMD_TEST_CONFIG=... cargo test -- --ignored`.
    //! Konfiguracja musi wskazywać bazę z sygnaturą EICAR (np. z `sigtool --md5`).

    use super::*;

    const EICAR: &str = r"X5O!P%@AP[4\PZX54(P^)7CC)7}$EICAR-STANDARD-ANTIVIRUS-TEST-FILE!$H+H*";

    #[tokio::test]
    #[ignore = "wymaga zainstalowanego clamd"]
    async fn detects_eicar_and_passes_clean_file() {
        let config = std::env::var("CLAMD_TEST_CONFIG").expect("CLAMD_TEST_CONFIG");
        let mut clamd = Clamd::new("/tmp/clamd.sock", config, Duration::from_secs(60));
        clamd.ensure_started().await.unwrap();

        let dir = std::env::temp_dir();
        let eicar = dir.join("scan-eicar.pdf");
        let clean = dir.join("scan-clean.jpg");
        tokio::fs::write(&eicar, EICAR).await.unwrap();
        tokio::fs::write(&clean, b"\xFF\xD8\xFF\xE0 not a virus")
            .await
            .unwrap();

        assert!(matches!(clamd.scan(&eicar).await, Verdict::Infected(_)));
        assert_eq!(clamd.scan(&clean).await, Verdict::Clean);
        assert!(matches!(
            clamd.scan(&dir.join("missing")).await,
            Verdict::Failed(_)
        ));
        assert!(clamd.version().await.starts_with("ClamAV"));

        // Drugi start w ciepłym środowisku używa działającego demona.
        clamd.ensure_started().await.unwrap();
    }
}
