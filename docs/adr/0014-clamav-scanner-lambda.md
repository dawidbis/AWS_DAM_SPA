# 0014. Skaner ClamAV jako obraz kontenera Lambda z clamd

- Status: zaakceptowany
- Data: 2026-10-02

## Kontekst

Każdy plik z kwarantanny musi przejść skan antywirusowy, zanim trafi do galerii (rozdział 7.2). ClamAV nie jest dostępny jako runtime Lambdy, a baza sygnatur ma ~300 MB i jej wczytanie trwa kilkadziesiąt sekund.

## Decyzja

- **Obraz kontenera** (`lambdas/pipeline/scan/container/Dockerfile`): `debian:bookworm-slim` + pakiety `clamav`, `clamav-daemon`, baza sygnatur pobrana przez `freshclam` przy budowaniu. Binarka `scan` w Ruście sama implementuje Lambda Runtime API, więc obraz bazowy AWS nie jest potrzebny.
- **clamd zamiast clamscan**: demon startuje przy pierwszym wywołaniu i zostaje w ciepłym środowisku, więc baza wczytuje się raz na środowisko, a nie przy każdym pliku. Komunikacja przez gniazdo `/tmp/clamd.sock` (`zPING`, `zVERSION`, `zSCAN`).
- **Fail closed**: `FOUND` → `INFECTED`; `OK` → `CLEAN_DRAFT`; wszystko inne (błąd, timeout, `Heuristics.Limits.*` przy przekroczonych limitach rozmiaru) → `SCAN_FAILED`. Limity clamd (`MaxFileSize`, `MaxScanSize`) obejmują cały dopuszczalny plik (1 GB), a `AlertExceedsMax` sprawia, że niepełny skan nie wygląda na czysty.
- **x86_64** dla tej jednej funkcji: obraz buduje się natywnie na runnerach GitHub, bez emulacji ARM (QEMU wydłużałby build kilkukrotnie). Pozostałe Lambdy zostają na arm64.
- **Aktualizacja sygnatur = przebudowa obrazu**: przy zmianie kodu skanera i co tydzień (`schedule` w `deploy.yml`). Zgodnie z rozdziałem 14 nie pobieramy sygnatur przy każdym uruchomieniu (limity serwerów ClamAV, czas cold startu).
- **Koszty**: 3008 MB pamięci (baza ~1,2 GB w RAM), `maximum_concurrency = 2` na wyzwalaczu SQS, ECR przechowuje 3 ostatnie obrazy.

## Rozważane alternatywy

- **GuardDuty Malware Protection for S3** — opłata za skanowany GB i brak kontroli nad silnikiem (ADR 0006 w planie).
- **libclamav przez FFI z Rusta** — jeden proces, ale `unsafe` i trudniejszy build (rozdział 8).
- **Sygnatury w S3 pobierane przy cold starcie** — szybsza aktualizacja bez przebudowy, ale dłuższy cold start; do rozważenia, jeśli tygodniowa aktualizacja okaże się za rzadka.

## Konsekwencje

- Cold start z wczytaniem bazy: rzędu 20–60 s (do zmierzenia i opisania w README).
- W etapie 1 skaner sam przenosi plik do `clean`/`infected`; w etapie 2 robią to osobne role uruchamiane przez Step Functions.
