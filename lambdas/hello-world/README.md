# `hello-world` — weryfikacja łańcucha build → deploy

Funkcja z etapu 0. Nie robi nic biznesowego: zwraca powitanie i wersję. Służy do sprawdzenia, że cały łańcuch działa — build Cargo Lambda (arm64), deploy przez Terraform z GitHub Actions i logowanie JSON w CloudWatch. Po każdym deployu wywołuje ją krok **Smoke test hello-world** w `deploy.yml`.

| | |
|---|---|
| Funkcja AWS | `matchday-dam-dev-hello-world` |
| Wyzwalacz | bezpośrednie wywołanie (`aws lambda invoke`), brak trasy w API |
| Rola IAM | `dam-hello-world` — tylko logi |
| Pamięć / timeout | 128 MB / 10 s |
| Kod | [`src/main.rs`](src/main.rs) |

## Kontrakt

```bash
aws lambda invoke --function-name matchday-dam-dev-hello-world \
  --cli-binary-format raw-in-base64-out --payload '{"name":"Matchday"}' out.json
cat out.json   # {"message":"Hello, Matchday!","version":"0.1.0"}
```

Pole `name` jest opcjonalne; pusty albo dłuższy niż 64 znaki → `KS Matchday`.

## Testy

`greets_by_name`, `falls_back_to_club_name`.
